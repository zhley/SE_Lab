//! 用户与会话模型（REQ-001 注册、REQ-002 登录、REQ-003 角色、REQ-012 封禁）。

use std::fmt;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::util;

/// 账号状态：正常 / 已封禁（REQ-012）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UserStatus {
    #[default]
    Normal,
    Banned,
}

impl UserStatus {
    pub fn label(self) -> &'static str {
        match self {
            UserStatus::Normal => "正常",
            UserStatus::Banned => "已封禁",
        }
    }
}

/// 会话角色，决定登录后进入哪个工作台（REQ-003）。
///
/// 同一账号可同时具备买家与卖家身份，管理员权限单独隔离。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Buyer,
    Seller,
    Admin,
}

impl Role {
    pub const ALL: [Role; 3] = [Role::Buyer, Role::Seller, Role::Admin];

    pub fn label(self) -> &'static str {
        match self {
            Role::Buyer => "买家",
            Role::Seller => "卖家",
            Role::Admin => "平台管理员",
        }
    }

    /// 用于持久化 / 日志的稳定标识。
    pub fn key(self) -> &'static str {
        match self {
            Role::Buyer => "buyer",
            Role::Seller => "seller",
            Role::Admin => "admin",
        }
    }

    pub fn parse(key: &str) -> Option<Role> {
        match key {
            "buyer" => Some(Role::Buyer),
            "seller" => Some(Role::Seller),
            "admin" => Some(Role::Admin),
            _ => None,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// 用户账号。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    /// 带随机盐的密码散列，永不存明文（REQ-027）。
    pub password_hash: String,
    pub status: UserStatus,
    /// 连续登录失败次数，达到 5 次触发锁定（REQ-002）。
    pub fail_count: u32,
    /// 锁定截止时间；`None` 表示未锁定。
    pub lock_until: Option<DateTime<Utc>>,
    pub buyer_role: bool,
    pub seller_role: bool,
    pub admin_role: bool,
    pub created_at: DateTime<Utc>,
}

impl User {
    /// 构造账号数据（不含任何格式/唯一性校验，校验在服务层完成）。
    pub fn new(
        username: impl Into<String>,
        phone: Option<String>,
        email: Option<String>,
        password_hash: impl Into<String>,
    ) -> Self {
        Self {
            user_id: util::new_id("U"),
            username: username.into(),
            phone,
            email,
            password_hash: password_hash.into(),
            status: UserStatus::Normal,
            fail_count: 0,
            lock_until: None,
            buyer_role: true,
            seller_role: true,
            admin_role: false,
            created_at: util::now(),
        }
    }

    pub fn is_banned(&self) -> bool {
        self.status == UserStatus::Banned
    }

    /// 以给定时刻判断是否处于登录锁定期（REQ-002）。
    pub fn is_locked(&self, now: DateTime<Utc>) -> bool {
        self.lock_until.is_some_and(|until| until > now)
    }

    pub fn has_role(&self, role: Role) -> bool {
        match role {
            Role::Buyer => self.buyer_role,
            Role::Seller => self.seller_role,
            Role::Admin => self.admin_role,
        }
    }

    /// 账号被授予的角色列表，供界面生成入口。
    pub fn roles(&self) -> Vec<Role> {
        Role::ALL
            .into_iter()
            .filter(|role| self.has_role(*role))
            .collect()
    }

    /// 锁定账号 `minutes` 分钟（REQ-002：连续失败 5 次锁定 15 分钟）。
    pub fn lock(&mut self, minutes: i64) {
        self.lock_until = Some(util::now() + Duration::minutes(minutes));
    }

    /// 管理员手动解锁（REQ-002）。
    pub fn unlock(&mut self) {
        self.fail_count = 0;
        self.lock_until = None;
    }
}

/// 登录会话。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub user_id: String,
    /// 当前会话使用的角色（同一账号可切换买家/卖家视角）。
    pub role: Role,
    pub expire_at: DateTime<Utc>,
}

impl Session {
    pub fn new(user_id: impl Into<String>, role: Role) -> Self {
        let now = util::now();
        Self {
            token: util::new_id("T"),
            user_id: user_id.into(),
            role,
            expire_at: now + Duration::hours(8),
        }
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expire_at <= now
    }
}

/// 注册验证码（REQ-028）：5 分钟内有效，使用一次后失效。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Captcha {
    /// 接收验证码的手机号或邮箱。
    pub target: String,
    pub code: String,
    pub created_at: DateTime<Utc>,
    pub expire_at: DateTime<Utc>,
    pub used: bool,
}

impl Captcha {
    /// 生成一张 5 分钟有效的验证码。
    pub fn new(target: impl Into<String>, code: impl Into<String>, ttl_minutes: i64) -> Self {
        let now = util::now();
        Self {
            target: target.into(),
            code: code.into(),
            created_at: now,
            expire_at: now + Duration::minutes(ttl_minutes),
            used: false,
        }
    }

    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        !self.used && self.expire_at > now
    }
}
