//! 预约模型（REQ-015 ~ REQ-019、REQ-021）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::util;

/// 预约状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReservationStatus {
    /// 待确认：等待卖家确认。
    Pending,
    /// 已确认：商品进入交易中。
    Confirmed,
    /// 已失效：被其他预约挤出、商品下架等。
    Invalid,
    /// 已取消：买家主动取消。
    Canceled,
}

impl ReservationStatus {
    pub fn label(self) -> &'static str {
        match self {
            ReservationStatus::Pending => "待确认",
            ReservationStatus::Confirmed => "已确认",
            ReservationStatus::Invalid => "已失效",
            ReservationStatus::Canceled => "已取消",
        }
    }

    /// 有效预约：待确认或已确认（REQ-016 判重、REQ-023 唯一性）。
    pub fn is_active(self) -> bool {
        matches!(
            self,
            ReservationStatus::Pending | ReservationStatus::Confirmed
        )
    }
}

/// 预约记录（购买意向）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reservation {
    pub reservation_id: String,
    pub item_id: String,
    pub buyer_id: String,
    pub seller_id: String,
    /// 提交预约时的商品版本号，用于状态冲突校验（REQ-021）。
    pub item_version: u64,
    pub status: ReservationStatus,
    /// 失效原因，界面展示用。
    pub invalid_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Reservation {
    pub fn new(item_id: impl Into<String>, buyer_id: impl Into<String>, seller_id: impl Into<String>, item_version: u64) -> Self {
        let now = util::now();
        Self {
            reservation_id: util::new_id("R"),
            item_id: item_id.into(),
            buyer_id: buyer_id.into(),
            seller_id: seller_id.into(),
            item_version,
            status: ReservationStatus::Pending,
            invalid_reason: None,
            created_at: now,
            updated_at: now,
        }
    }
}
