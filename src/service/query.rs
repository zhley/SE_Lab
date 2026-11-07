//! 查询与统计服务（REQ-013、REQ-014、REQ-024）。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::domain::item::{Item, ItemStatus};
use crate::domain::user::Session;
use crate::error::Result;
use crate::repo::Store;
use crate::service::not_implemented;

/// 商品查询条件。
#[derive(Debug, Clone, Default)]
pub struct ItemFilter {
    /// 关键词，匹配标题与描述。
    pub keyword: Option<String>,
    pub category_id: Option<String>,
    pub price_min: Option<Decimal>,
    pub price_max: Option<Decimal>,
    pub status: Option<ItemStatus>,
}

/// 排序字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortBy {
    #[default]
    CreatedAt,
    Price,
    Sales,
}

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

/// 分页结果。
#[derive(Debug, Clone, Default)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: u32,
    pub page_size: u32,
}

impl<T> Page<T> {
    pub fn total_pages(&self) -> u32 {
        if self.page_size == 0 {
            0
        } else {
            (self.total as u32).div_ceil(self.page_size)
        }
    }
}

/// 多条件查询 + 排序分页，并按角色过滤可见范围（REQ-013、REQ-014）。
pub fn search_items(
    _store: &Store,
    _session: &Session,
    _filter: &ItemFilter,
    _sort: SortBy,
    _order: SortOrder,
    _page: u32,
) -> Result<Page<Item>> {
    not_implemented("service::query::search_items")
}

/// 卖家统计：在售 / 已售 / 下架数量与预约数量（REQ-024）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SellerStats {
    pub published: usize,
    pub sold: usize,
    pub offline: usize,
    pub reservations: usize,
}

pub fn seller_stats(_store: &Store, _seller_id: &str) -> Result<SellerStats> {
    not_implemented("service::query::seller_stats")
}

/// 管理员统计：待审核数、违规数、按分类的已售统计（REQ-024）。
#[derive(Debug, Clone, Default)]
pub struct AdminStats {
    pub pending_items: usize,
    pub violation_items: usize,
    pub sold_by_category: Vec<(String, usize)>,
}

pub fn admin_stats(_store: &Store) -> Result<AdminStats> {
    not_implemented("service::query::admin_stats")
}
