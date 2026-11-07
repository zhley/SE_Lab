//! 用户与权限服务（REQ-001、REQ-002、REQ-003、REQ-012、REQ-027、REQ-028、REQ-029）。

use crate::domain::user::{Session, User};
use crate::error::Result;
use crate::repo::Store;
use crate::service::not_implemented;

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

/// 注册新账号：校验唯一性、密码规则、验证码，写入用户并对密码加盐散列。
pub fn register(_store: &mut Store, _req: &RegisterRequest) -> Result<User> {
    not_implemented("service::user::register")
}

/// 登录：支持用户名 / 手机号 / 邮箱，处理失败计数、锁定与封禁（REQ-002）。
pub fn login(_store: &mut Store, _account: &str, _password: &str) -> Result<Session> {
    not_implemented("service::user::login")
}

/// 注销会话。
pub fn logout(_store: &mut Store, _token: &str) -> Result<()> {
    not_implemented("service::user::logout")
}

/// 发放验证码（REQ-028：5 分钟有效、60 秒内不可重复获取）。
pub fn issue_captcha(_store: &mut Store, _target: &str) -> Result<String> {
    not_implemented("service::user::issue_captcha")
}

/// 校验会话并返回当前用户，同时校验角色（REQ-029 越权访问防护）。
pub fn authenticate<'a>(
    _store: &'a Store,
    _session: &Session,
    _role: Option<crate::domain::user::Role>,
) -> Result<&'a User> {
    not_implemented("service::user::authenticate")
}

/// 封禁用户：记录原因，名下已发布商品转已下架（REQ-012）。
pub fn ban_user(_store: &mut Store, _admin_id: &str, _user_id: &str, _reason: &str) -> Result<()> {
    not_implemented("service::user::ban_user")
}

/// 解封用户：必须填写理由（REQ-012）。
pub fn unban_user(_store: &mut Store, _admin_id: &str, _user_id: &str, _reason: &str) -> Result<()> {
    not_implemented("service::user::unban_user")
}

/// 管理员手动解锁登录（REQ-002）。
pub fn unlock_login(_store: &mut Store, _admin_id: &str, _user_id: &str) -> Result<()> {
    not_implemented("service::user::unlock_login")
}
