//! 数据层：内存数据存储与查询辅助。
//!
//! 骨架阶段使用内存集合承载全部业务数据，后续通过
//! [`persistence::Persistence`] 接入文件持久化，业务层接口不随之改变。

pub mod persistence;

use serde::{Deserialize, Serialize};

use crate::domain::audit::{Appeal, AuditRecord, ViolationRecord};
use crate::domain::item::{Category, Item};
use crate::domain::log::OperationLog;
use crate::domain::notice::Notification;
use crate::domain::reservation::Reservation;
use crate::domain::trade::Trade;
use crate::domain::user::{Session, User};

/// 全量业务数据。
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    pub users: Vec<User>,
    pub sessions: Vec<Session>,
    pub categories: Vec<Category>,
    pub items: Vec<Item>,
    pub reservations: Vec<Reservation>,
    pub trades: Vec<Trade>,
    pub audits: Vec<AuditRecord>,
    pub violations: Vec<ViolationRecord>,
    pub appeals: Vec<Appeal>,
    pub notifications: Vec<Notification>,
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
        self.users.iter().find(|u| {
            u.username == account
                || u.phone.as_deref() == Some(account)
                || u.email.as_deref() == Some(account)
        })
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
