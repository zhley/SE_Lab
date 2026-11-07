//! 交易模型（REQ-020）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::util;

/// 交易状态。
///
/// 需求中交易记录只存在“已完成”一种业务状态（REQ-020）：
/// 卖家标记完成即生成已完成交易，买家确认仅做标记，不改变状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeStatus {
    Completed,
}

impl TradeStatus {
    pub fn label(self) -> &'static str {
        match self {
            TradeStatus::Completed => "已完成",
        }
    }
}

/// 交易记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub trade_id: String,
    pub item_id: String,
    pub reservation_id: String,
    pub buyer_id: String,
    pub seller_id: String,
    pub status: TradeStatus,
    /// 买家确认标记（REQ-020），不影响交易与商品状态。
    pub buyer_confirmed: bool,
    /// 卖家标记完成的时间。
    pub completed_at: Option<DateTime<Utc>>,
    /// 买家确认截止时间：完成起 7 天未确认则自动确认。
    pub confirm_deadline: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Trade {
    pub fn new(
        reservation_id: impl Into<String>,
        item_id: impl Into<String>,
        buyer_id: impl Into<String>,
        seller_id: impl Into<String>,
    ) -> Self {
        Self {
            trade_id: util::new_id("D"),
            item_id: item_id.into(),
            reservation_id: reservation_id.into(),
            buyer_id: buyer_id.into(),
            seller_id: seller_id.into(),
            status: TradeStatus::Completed,
            buyer_confirmed: false,
            completed_at: None,
            confirm_deadline: None,
            created_at: util::now(),
        }
    }
}
