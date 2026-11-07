//! 数据层：业务数据集合、查询辅助与持久化。
//!
//! 骨架阶段以内存集合承载全部业务数据，通过
//! [`persistence::JsonFilePersistence`] 全量写入 JSON 文件。

pub mod persistence;
pub mod seed;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::audit::{Appeal, AuditRecord, ViolationRecord};
use crate::domain::item::{Category, Item};
use crate::domain::log::OperationLog;
use crate::domain::notice::Notification;
use crate::domain::reservation::Reservation;
use crate::domain::trade::Trade;
use crate::domain::user::{Captcha, Session, User};

/// 默认数据文件位置（相对工作目录），可用环境变量 `SE_LAB_DATA` 覆盖。
pub fn default_data_path() -> PathBuf {
    std::env::var_os("SE_LAB_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data").join("store.json"))
}

/// 全量业务数据。
///
/// 所有字段都带 `serde(default)`，便于旧数据文件在新增字段后仍能加载。
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default)]
    pub sessions: Vec<Session>,
    #[serde(default)]
    pub captchas: Vec<Captcha>,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub reservations: Vec<Reservation>,
    #[serde(default)]
    pub trades: Vec<Trade>,
    #[serde(default)]
    pub audits: Vec<AuditRecord>,
    #[serde(default)]
    pub violations: Vec<ViolationRecord>,
    #[serde(default)]
    pub appeals: Vec<Appeal>,
    #[serde(default)]
    pub notifications: Vec<Notification>,
    #[serde(default)]
    pub logs: Vec<OperationLog>,
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    // ---------- 用户 ----------

    pub fn user(&self, user_id: &str) -> Option<&User> {
        self.users.iter().find(|u| u.user_id == user_id)
    }

    pub fn user_mut(&mut self, user_id: &str) -> Option<&mut User> {
        self.users.iter_mut().find(|u| u.user_id == user_id)
    }

    /// 按用户名 / 手机号 / 邮箱查找账号（REQ-002 三种登录方式）。
    pub fn find_user_by_account(&self, account: &str) -> Option<&User> {
        let account = account.trim();
        self.users.iter().find(|u| {
            u.username.eq_ignore_ascii_case(account)
                || u.phone.as_deref() == Some(account)
                || u.email.as_deref() == Some(account)
        })
    }

    pub fn find_user_by_username(&self, username: &str) -> Option<&User> {
        self.users
            .iter()
            .find(|u| u.username.eq_ignore_ascii_case(username.trim()))
    }

    /// 该手机号是否已被绑定。
    pub fn phone_taken(&self, phone: &str) -> bool {
        self.users.iter().any(|u| u.phone.as_deref() == Some(phone))
    }

    /// 该邮箱是否已被绑定。
    pub fn email_taken(&self, email: &str) -> bool {
        self.users.iter().any(|u| u.email.as_deref() == Some(email))
    }

    // ---------- 商品 ----------

    pub fn item(&self, item_id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.item_id == item_id)
    }

    pub fn item_mut(&mut self, item_id: &str) -> Option<&mut Item> {
        self.items.iter_mut().find(|i| i.item_id == item_id)
    }

    pub fn items_of_seller(&self, seller_id: &str) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(move |i| i.seller_id == seller_id)
    }

    // ---------- 预约 ----------

    pub fn reservation(&self, reservation_id: &str) -> Option<&Reservation> {
        self.reservations
            .iter()
            .find(|r| r.reservation_id == reservation_id)
    }

    pub fn reservation_mut(&mut self, reservation_id: &str) -> Option<&mut Reservation> {
        self.reservations
            .iter_mut()
            .find(|r| r.reservation_id == reservation_id)
    }

    /// 某买家对某商品的有效预约（REQ-016 / REQ-023）。
    pub fn active_reservation(&self, item_id: &str, buyer_id: &str) -> Option<&Reservation> {
        self.reservations
            .iter()
            .find(|r| r.item_id == item_id && r.buyer_id == buyer_id && r.status.is_active())
    }

    // ---------- 交易 ----------

    pub fn trade(&self, trade_id: &str) -> Option<&Trade> {
        self.trades.iter().find(|t| t.trade_id == trade_id)
    }

    // ---------- 通知 ----------

    pub fn notifications_of(&self, user_id: &str) -> impl Iterator<Item = &Notification> {
        self.notifications
            .iter()
            .filter(move |n| n.user_id == user_id)
    }

    /// 未读通知数（REQ-033：登录后顶部展示未读数）。
    pub fn unread_count(&self, user_id: &str) -> usize {
        self.notifications_of(user_id).filter(|n| !n.read).count()
    }
}
