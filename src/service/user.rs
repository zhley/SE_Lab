//! 用户与权限服务（REQ-001、REQ-002、REQ-003、REQ-012、REQ-027、REQ-028、REQ-029）。

use crate::domain::EntityKind;
use crate::domain::item::ItemStatus;
use crate::domain::log::{ActionType, LogResult};
use crate::domain::user::{Captcha, Role, Session, User, UserStatus};
use crate::error::{AppError, Result};
use crate::repo::Store;
use crate::security;
use crate::service::item;
use crate::service::log::{self, NewLog};
use crate::service::trade;
use crate::util;

/// 连续登录失败多少次后锁定（REQ-002）。
pub const MAX_FAIL_COUNT: u32 = 5;
/// 锁定分钟数（REQ-002）。
pub const LOCK_MINUTES: i64 = 15;
/// 验证码有效期（REQ-028）。
pub const CAPTCHA_TTL_MINUTES: i64 = 5;
/// 验证码重发间隔（REQ-028）。
pub const CAPTCHA_RESEND_SECONDS: i64 = 60;

/// 统一的登录失败提示，不区分账号不存在与密码错误（REQ-027）。
const INVALID_CREDENTIALS: &str = "用户名或密码错误";

/// 注册请求。
#[derive(Debug, Clone, Default)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    /// 手机号与邮箱至少填写一项（REQ-001）。
    pub phone: Option<String>,
    pub email: Option<String>,
    /// 验证码（REQ-028）。
    pub captcha: String,
}

/// 登录前对账号状态的快照，避免后续可变借用冲突。
struct LoginSnapshot {
    user_id: String,
    banned: bool,
    lock_until: Option<chrono::DateTime<chrono::Utc>>,
    password_ok: bool,
    is_admin: bool,
    is_buyer: bool,
}

/// 注册新账号：校验唯一性、密码规则、验证码，写入用户并对密码加盐散列。
pub fn register(store: &mut Store, req: &RegisterRequest) -> Result<User> {
    let username = req.username.trim().to_string();
    security::validate_username(&username)?;
    security::validate_password(&req.password)?;

    let phone = non_empty(req.phone.as_deref());
    let email = non_empty(req.email.as_deref());
    if phone.is_none() && email.is_none() {
        return Err(AppError::InvalidInput(
            "手机号与邮箱至少填写一项".into(),
        ));
    }
    if let Some(phone) = &phone {
        security::validate_phone(phone)?;
    }
    if let Some(email) = &email {
        security::validate_email(email)?;
    }

    // 唯一性校验（REQ-023）。
    if store.find_user_by_username(&username).is_some() {
        return Err(AppError::Duplicate(format!("用户名「{username}」已被占用")));
    }
    if let Some(phone) = &phone
        && store.phone_taken(phone)
    {
        return Err(AppError::Duplicate("该手机号已被其他账号绑定".into()));
    }
    if let Some(email) = &email
        && store.email_taken(email)
    {
        return Err(AppError::Duplicate("该邮箱已被其他账号绑定".into()));
    }

    // 验证码校验（REQ-028），接收目标为手机号，其次邮箱。
    let target = phone.clone().or_else(|| email.clone()).unwrap_or_default();
    consume_captcha(store, &target, &req.captcha)?;

    let user = User::new(
        username,
        phone,
        email,
        security::hash_password(&req.password),
    );

    log::record(
        store,
        &NewLog {
            operator_id: &user.user_id,
            role: Role::Buyer,
            action: ActionType::Register,
            target_type: EntityKind::User,
            target_id: &user.user_id,
            result: LogResult::Success,
            detail: "注册成功",
        },
    )?;

    store.users.push(user.clone());
    Ok(user)
}

/// 登录：支持用户名 / 手机号 / 邮箱，处理失败计数、锁定与封禁（REQ-002）。
pub fn login(store: &mut Store, account: &str, password: &str) -> Result<Session> {
    let account = account.trim();
    if account.is_empty() {
        return Err(AppError::InvalidInput("请输入账号".into()));
    }
    if password.is_empty() {
        return Err(AppError::InvalidInput("请输入密码".into()));
    }

    let now = util::now();
    let snapshot = {
        // 账号不存在与密码错误使用同一提示（REQ-027）。
        let user = store
            .find_user_by_account(account)
            .ok_or_else(|| AppError::InvalidInput(INVALID_CREDENTIALS.into()))?;
        LoginSnapshot {
            user_id: user.user_id.clone(),
            banned: user.is_banned(),
            lock_until: user.lock_until.filter(|until| *until > now),
            password_ok: security::verify_password(password, &user.password_hash),
            is_admin: user.admin_role,
            is_buyer: user.buyer_role,
        }
    };

    if snapshot.banned {
        record_login_failure(store, &snapshot.user_id, "账号已被封禁")?;
        return Err(AppError::PermissionDenied(
            "账号已被封禁，无法登录".into(),
        ));
    }

    if let Some(until) = snapshot.lock_until {
        let seconds_left = (until - now).num_seconds().max(0);
        let minutes = (seconds_left + 59) / 60;
        return Err(AppError::PermissionDenied(format!(
            "账号处于登录锁定期，请约 {minutes} 分钟后再试"
        )));
    }

    if !snapshot.password_ok {
        let locked = lock_after_failure(store, &snapshot.user_id)?;
        record_login_failure(store, &snapshot.user_id, "密码错误")?;
        return Err(AppError::InvalidInput(if locked {
            format!("密码错误次数过多，账号已锁定 {LOCK_MINUTES} 分钟")
        } else {
            INVALID_CREDENTIALS.to_string()
        }));
    }

    // 登录成功：清空失败计数，单账号只保留一个会话。
    if let Some(user) = store.user_mut(&snapshot.user_id) {
        user.fail_count = 0;
        user.lock_until = None;
    }
    store.sessions.retain(|session| session.user_id != snapshot.user_id);

    // 管理员进后台；普通账号默认以买家视角进入，可在工作台切换角色。
    let role = if snapshot.is_admin {
        Role::Admin
    } else if snapshot.is_buyer {
        Role::Buyer
    } else {
        Role::Seller
    };
    let session = Session::new(snapshot.user_id.clone(), role);
    store.sessions.push(session.clone());

    log::record(
        store,
        &NewLog {
            operator_id: &snapshot.user_id,
            role,
            action: ActionType::Login,
            target_type: EntityKind::User,
            target_id: &snapshot.user_id,
            result: LogResult::Success,
            detail: "登录成功",
        },
    )?;

    Ok(session)
}

/// 注销会话。
pub fn logout(store: &mut Store, token: &str) -> Result<()> {
    let before = store.sessions.len();
    store.sessions.retain(|session| session.token != token);
    if store.sessions.len() == before {
        return Err(AppError::NotFound("会话不存在或已失效".into()));
    }
    Ok(())
}

/// 发放验证码（REQ-028：5 分钟有效、60 秒内不可重复获取）。
///
/// 演示环境不发送短信 / 邮件，验证码直接返回给界面展示。
pub fn issue_captcha(store: &mut Store, target: &str) -> Result<String> {
    let target = target.trim();
    if target.is_empty() {
        return Err(AppError::InvalidInput(
            "请先填写手机号或邮箱".into(),
        ));
    }
    if target.contains('@') {
        security::validate_email(target)?;
    } else {
        security::validate_phone(target)?;
    }

    let now = util::now();
    if let Some(last) = store
        .captchas
        .iter()
        .filter(|captcha| captcha.target == target)
        .max_by_key(|captcha| captcha.created_at)
    {
        let elapsed = (now - last.created_at).num_seconds();
        if elapsed < CAPTCHA_RESEND_SECONDS {
            return Err(AppError::InvalidInput(format!(
                "获取过于频繁，请 {} 秒后重试",
                CAPTCHA_RESEND_SECONDS - elapsed
            )));
        }
    }

    let code = security::generate_captcha_code();
    store
        .captchas
        .push(Captcha::new(target, &code, CAPTCHA_TTL_MINUTES));
    Ok(code)
}

/// 校验会话并返回当前用户，同时校验角色（REQ-029 越权访问防护）。
pub fn authenticate<'a>(
    store: &'a Store,
    session: &Session,
    role: Option<Role>,
) -> Result<&'a User> {
    let now = util::now();
    let stored = store
        .sessions
        .iter()
        .find(|item| item.token == session.token)
        .ok_or_else(|| AppError::PermissionDenied("登录状态已失效，请重新登录".into()))?;

    if stored.is_expired(now) {
        return Err(AppError::PermissionDenied("登录已过期，请重新登录".into()));
    }

    let user = store
        .user(&stored.user_id)
        .ok_or_else(|| AppError::NotFound("账号不存在".into()))?;

    if user.is_banned() {
        return Err(AppError::PermissionDenied("账号已被封禁".into()));
    }

    if let Some(role) = role
        && !user.has_role(role)
    {
        return Err(AppError::PermissionDenied(format!(
            "无权限执行{}操作",
            role.label()
        )));
    }

    Ok(user)
}

/// 封禁用户：记录原因，名下已发布商品转已下架、待确认预约失效（REQ-012）。
pub fn ban_user(store: &mut Store, admin_id: &str, user_id: &str, reason: &str) -> Result<()> {
    require_admin(store, admin_id)?;

    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::InvalidInput("封禁必须填写理由".into()));
    }

    let already_banned = {
        let user = store
            .user(user_id)
            .ok_or_else(|| AppError::NotFound("目标用户不存在".into()))?;
        user.status == UserStatus::Banned
    };
    // 重复封禁不生成重复记录。
    if already_banned {
        return Ok(());
    }

    {
        let user = store
            .user_mut(user_id)
            .ok_or_else(|| AppError::NotFound("目标用户不存在".into()))?;
        user.status = UserStatus::Banned;
        user.fail_count = 0;
        user.lock_until = None;
    }
    // 被封禁账号立即下线。
    store.sessions.retain(|session| session.user_id != user_id);

    // 名下已发布商品全部转为已下架，其待确认预约失效（REQ-012）。
    let published_items: Vec<String> = store
        .items_of_seller(user_id)
        .filter(|item| item.status == ItemStatus::Published)
        .map(|item| item.item_id.clone())
        .collect();

    for item_id in published_items {
        item::force_status(store, &item_id, ItemStatus::Offline)?;
        trade::invalidate_pending_reservations(store, &item_id, "卖家已被封禁，商品已下架")?;
    }

    log::record(
        store,
        &NewLog {
            operator_id: admin_id,
            role: Role::Admin,
            action: ActionType::BanUser,
            target_type: EntityKind::User,
            target_id: user_id,
            result: LogResult::Success,
            detail: reason,
        },
    )?;

    Ok(())
}

/// 解封用户：必须填写理由（REQ-012）。
pub fn unban_user(store: &mut Store, admin_id: &str, user_id: &str, reason: &str) -> Result<()> {
    require_admin(store, admin_id)?;

    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::InvalidInput("解封必须填写理由".into()));
    }

    let banned = {
        let user = store
            .user(user_id)
            .ok_or_else(|| AppError::NotFound("目标用户不存在".into()))?;
        user.is_banned()
    };
    if !banned {
        return Ok(());
    }

    if let Some(user) = store.user_mut(user_id) {
        user.status = UserStatus::Normal;
    }

    log::record(
        store,
        &NewLog {
            operator_id: admin_id,
            role: Role::Admin,
            action: ActionType::UnbanUser,
            target_type: EntityKind::User,
            target_id: user_id,
            result: LogResult::Success,
            detail: reason,
        },
    )?;

    Ok(())
}

/// 管理员手动解锁登录（REQ-002）。
pub fn unlock_login(store: &mut Store, admin_id: &str, user_id: &str) -> Result<()> {
    require_admin(store, admin_id)?;

    let user = store
        .user_mut(user_id)
        .ok_or_else(|| AppError::NotFound("目标用户不存在".into()))?;
    user.unlock();

    log::record(
        store,
        &NewLog {
            operator_id: admin_id,
            role: Role::Admin,
            action: ActionType::UnlockLogin,
            target_type: EntityKind::User,
            target_id: user_id,
            result: LogResult::Success,
            detail: "管理员手动解锁",
        },
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// 内部辅助
// ---------------------------------------------------------------------------

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// 校验验证码并标记为已使用（REQ-028：一次有效）。
fn consume_captcha(store: &mut Store, target: &str, code: &str) -> Result<()> {
    let code = code.trim();
    if code.is_empty() {
        return Err(AppError::InvalidInput("请填写验证码".into()));
    }

    let now = util::now();
    let captcha = store
        .captchas
        .iter_mut()
        .filter(|captcha| captcha.target == target && !captcha.used)
        .max_by_key(|captcha| captcha.created_at)
        .ok_or_else(|| AppError::InvalidInput("验证码不存在，请先获取验证码".into()))?;

    if !captcha.is_valid(now) {
        return Err(AppError::InvalidInput(
            "验证码已过期，请重新获取".into(),
        ));
    }
    if captcha.code != code {
        return Err(AppError::InvalidInput("验证码错误".into()));
    }

    captcha.used = true;
    Ok(())
}

/// 失败计数 +1，达到阈值则锁定，返回本次是否触发锁定。
fn lock_after_failure(store: &mut Store, user_id: &str) -> Result<bool> {
    let user = store
        .user_mut(user_id)
        .ok_or_else(|| AppError::NotFound("账号不存在".into()))?;
    user.fail_count += 1;
    if user.fail_count >= MAX_FAIL_COUNT {
        user.lock(LOCK_MINUTES);
        user.fail_count = 0;
        return Ok(true);
    }
    Ok(false)
}

fn record_login_failure(store: &mut Store, user_id: &str, detail: &str) -> Result<()> {
    log::record(
        store,
        &NewLog {
            operator_id: user_id,
            role: Role::Buyer,
            action: ActionType::LoginFailed,
            target_type: EntityKind::User,
            target_id: user_id,
            result: LogResult::Failure,
            detail,
        },
    )
}

/// 仅管理员可执行后台操作（REQ-003、REQ-029）。
fn require_admin(store: &Store, operator_id: &str) -> Result<()> {
    let operator = store
        .user(operator_id)
        .ok_or_else(|| AppError::NotFound("操作人账号不存在".into()))?;
    if !operator.admin_role {
        return Err(AppError::PermissionDenied("仅管理员可以执行该操作".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed;

    const PHONE: &str = "13900000001";

    fn request(username: &str, captcha: &str) -> RegisterRequest {
        RegisterRequest {
            username: username.into(),
            password: "abcd1234".into(),
            phone: Some(PHONE.into()),
            email: None,
            captcha: captcha.into(),
        }
    }

    fn store_with_user(username: &str) -> (Store, String) {
        let mut store = Store::new();
        let code = issue_captcha(&mut store, PHONE).expect("发放验证码失败");
        let user = register(&mut store, &request(username, &code)).expect("注册失败");
        (store, user.user_id)
    }

    #[test]
    fn register_then_login_by_username_and_phone() {
        let (mut store, user_id) = store_with_user("buyer01");

        let session = login(&mut store, "buyer01", "abcd1234").expect("用户名登录失败");
        assert_eq!(session.user_id, user_id);
        assert_eq!(session.role, Role::Buyer);

        let session = login(&mut store, PHONE, "abcd1234").expect("手机号登录失败");
        assert_eq!(session.user_id, user_id);
        // 单账号只保留最新会话。
        assert_eq!(store.sessions.len(), 1);
    }

    #[test]
    fn register_requires_phone_or_email() {
        let mut store = Store::new();
        let code = issue_captcha(&mut store, PHONE).expect("发放验证码失败");
        let mut req = request("buyer01", &code);
        req.phone = None;
        assert!(matches!(
            register(&mut store, &req),
            Err(AppError::InvalidInput(_))
        ));
        assert!(store.users.is_empty());
    }

    #[test]
    fn duplicate_username_and_phone_rejected() {
        let (mut store, _) = store_with_user("buyer01");

        let code = issue_captcha(&mut store, "13900000002").unwrap();
        let mut req = request("buyer01", &code);
        req.phone = Some("13900000002".into());
        assert!(matches!(
            register(&mut store, &req),
            Err(AppError::Duplicate(_))
        ));

        // 手机号已被占用：直接放入验证码，绕过 60 秒重发限制。
        store
            .captchas
            .push(Captcha::new(PHONE, "123456", CAPTCHA_TTL_MINUTES));
        let req = request("buyer02", "123456");
        assert!(matches!(
            register(&mut store, &req),
            Err(AppError::Duplicate(_))
        ));
        assert_eq!(store.users.len(), 1);
    }

    #[test]
    fn captcha_must_match_and_single_use() {
        let mut store = Store::new();
        let code = issue_captcha(&mut store, PHONE).unwrap();

        assert!(matches!(
            register(&mut store, &request("buyer01", "000000")),
            Err(AppError::InvalidInput(_))
        ));

        register(&mut store, &request("buyer01", &code)).expect("注册失败");
        // 同一验证码不能再次使用。
        let mut req = request("buyer02", &code);
        req.phone = Some("13900000002".into());
        assert!(matches!(
            register(&mut store, &req),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn captcha_resend_is_throttled() {
        let mut store = Store::new();
        issue_captcha(&mut store, PHONE).unwrap();
        let err = issue_captcha(&mut store, PHONE).unwrap_err();
        assert!(err.message().contains("频繁"));
    }

    #[test]
    fn account_locks_after_five_failures() {
        let (mut store, user_id) = store_with_user("buyer01");

        for attempt in 1..=MAX_FAIL_COUNT {
            let err = login(&mut store, "buyer01", "wrong1234").unwrap_err();
            if attempt < MAX_FAIL_COUNT {
                assert_eq!(err.message(), INVALID_CREDENTIALS);
            } else {
                assert!(err.message().contains("锁定"));
            }
        }

        let user = store.user(&user_id).unwrap();
        assert!(user.is_locked(util::now()));

        // 锁定期间即使密码正确也不能登录（REQ-002）。
        let err = login(&mut store, "buyer01", "abcd1234").unwrap_err();
        assert!(err.message().contains("锁定"));
    }

    #[test]
    fn admin_unlock_allows_login_again() {
        let (mut store, user_id) = store_with_user("buyer01");
        for _ in 0..MAX_FAIL_COUNT {
            let _ = login(&mut store, "buyer01", "wrong1234");
        }
        assert!(login(&mut store, "buyer01", "abcd1234").is_err());

        let admin_id = store.find_user_by_username("admin").map(|u| u.user_id.clone());
        let admin_id = match admin_id {
            Some(id) => id,
            None => {
                // 未使用演示数据时手工补一个管理员。
                let mut admin = User::new("root", None, None, "x");
                admin.admin_role = true;
                admin.buyer_role = false;
                admin.seller_role = false;
                let id = admin.user_id.clone();
                store.users.push(admin);
                id
            }
        };

        unlock_login(&mut store, &admin_id, &user_id).expect("解锁失败");
        login(&mut store, "buyer01", "abcd1234").expect("解锁后应能登录");
    }

    #[test]
    fn banned_user_cannot_login_and_sessions_are_dropped() {
        let mut store = seed::demo_store();
        let admin_id = store.find_user_by_username("admin").unwrap().user_id.clone();
        let buyer_id = store.find_user_by_username("buyer").unwrap().user_id.clone();
        let session = login(&mut store, "buyer", "buyer123").unwrap();
        assert!(authenticate(&store, &session, None).is_ok());

        ban_user(&mut store, &admin_id, &buyer_id, "多次发布违规商品").expect("封禁失败");

        assert!(login(&mut store, "buyer", "buyer123").is_err());
        assert!(authenticate(&store, &session, None).is_err());
        assert!(store.sessions.iter().all(|s| s.user_id != buyer_id));
    }

    #[test]
    fn non_admin_cannot_ban() {
        let mut store = seed::demo_store();
        let seller_id = store.find_user_by_username("seller").unwrap().user_id.clone();
        let buyer_id = store.find_user_by_username("buyer").unwrap().user_id.clone();
        assert!(matches!(
            ban_user(&mut store, &seller_id, &buyer_id, "看你不顺眼"),
            Err(AppError::PermissionDenied(_))
        ));
    }

    #[test]
    fn logout_invalidates_session() {
        let (mut store, _) = store_with_user("buyer01");
        let session = login(&mut store, "buyer01", "abcd1234").unwrap();
        logout(&mut store, &session.token).expect("注销失败");
        assert!(authenticate(&store, &session, None).is_err());
        assert!(logout(&mut store, &session.token).is_err());
    }

    #[test]
    fn role_check_rejects_other_role() {
        let (mut store, _) = store_with_user("buyer01");
        let session = login(&mut store, "buyer01", "abcd1234").unwrap();
        assert!(authenticate(&store, &session, Some(Role::Seller)).is_ok());
        assert!(matches!(
            authenticate(&store, &session, Some(Role::Admin)),
            Err(AppError::PermissionDenied(_))
        ));
    }
}
