//! 商品模型（REQ-005 ~ REQ-011、REQ-013、REQ-014）。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::util;

/// 商品状态：八种（REQ-008）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemStatus {
    /// 草稿：仅卖家可见，可反复编辑。
    Draft,
    /// 待审核：审核期间买家不可见。
    Pending,
    /// 已发布：买家可见、可预约。
    Published,
    /// 交易中：已确认预约，不可修改/下架。
    InTrade,
    /// 已售出：终态。
    Sold,
    /// 已下架：卖家主动下架或卖家被封禁。
    Offline,
    /// 违规下架：管理员处理，可申诉。
    Violation,
    /// 审核驳回：卖家可修改后重新提交。
    Rejected,
}

impl ItemStatus {
    pub const ALL: [ItemStatus; 8] = [
        ItemStatus::Draft,
        ItemStatus::Pending,
        ItemStatus::Published,
        ItemStatus::InTrade,
        ItemStatus::Sold,
        ItemStatus::Offline,
        ItemStatus::Violation,
        ItemStatus::Rejected,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ItemStatus::Draft => "草稿",
            ItemStatus::Pending => "待审核",
            ItemStatus::Published => "已发布",
            ItemStatus::InTrade => "交易中",
            ItemStatus::Sold => "已售出",
            ItemStatus::Offline => "已下架",
            ItemStatus::Violation => "违规下架",
            ItemStatus::Rejected => "审核驳回",
        }
    }

    /// 买家列表中可见的状态（REQ-014）。
    pub fn visible_to_buyer(self) -> bool {
        matches!(self, ItemStatus::Published)
    }

    /// 卖家可编辑内容的状态。
    pub fn editable(self) -> bool {
        matches!(
            self,
            ItemStatus::Draft | ItemStatus::Rejected | ItemStatus::Published | ItemStatus::Offline
        )
    }
}

/// 允许的状态转换表（REQ-008、状态图）。
///
/// 转换校验由 `service::item::change_status` 依据该表完成，
/// 表外的转换一律拒绝。
pub const ALLOWED_TRANSITIONS: &[(ItemStatus, ItemStatus)] = &[
    (ItemStatus::Draft, ItemStatus::Pending),
    (ItemStatus::Pending, ItemStatus::Published),
    (ItemStatus::Pending, ItemStatus::Rejected),
    (ItemStatus::Rejected, ItemStatus::Pending),
    // REQ-009：已发布商品修改后必须重新进入待审核。
    (ItemStatus::Published, ItemStatus::Pending),
    (ItemStatus::Published, ItemStatus::InTrade),
    (ItemStatus::Published, ItemStatus::Offline),
    (ItemStatus::Published, ItemStatus::Violation),
    (ItemStatus::InTrade, ItemStatus::Sold),
    (ItemStatus::InTrade, ItemStatus::Published),
    (ItemStatus::Offline, ItemStatus::Pending),
    (ItemStatus::Violation, ItemStatus::Pending),
];

/// 商品。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub item_id: String,
    pub seller_id: String,
    pub category_id: String,
    pub title: String,
    pub description: String,
    pub price: Decimal,
    pub status: ItemStatus,
    /// 乐观并发版本号，冲突时提示“状态已变化，请刷新”（REQ-021）。
    pub version: u64,
    /// 最近一次审核驳回理由（REQ-007）。
    pub reject_reason: Option<String>,
    pub images: Vec<ItemImage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Item {
    pub fn new(seller_id: impl Into<String>, category_id: impl Into<String>) -> Self {
        let now = util::now();
        Self {
            item_id: util::new_id("I"),
            seller_id: seller_id.into(),
            category_id: category_id.into(),
            title: String::new(),
            description: String::new(),
            price: Decimal::ZERO,
            status: ItemStatus::Draft,
            version: 1,
            reject_reason: None,
            images: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// 商品图片。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemImage {
    pub image_id: String,
    pub url: String,
    pub sort: u32,
}

/// 商品分类。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub category_id: String,
    pub name: String,
}

impl Category {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            category_id: util::new_id("C"),
            name: name.into(),
        }
    }
}
