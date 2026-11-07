//! 查询与统计服务（REQ-013、REQ-014、REQ-024）。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::domain::audit::Appeal;
use crate::domain::item::{Category, Item, ItemStatus};
use crate::domain::reservation::{Reservation, ReservationStatus};
use crate::domain::trade::Trade;
use crate::domain::user::{Role, Session, User};
use crate::error::Result;
use crate::repo::Store;

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
            self.total.div_ceil(self.page_size as usize) as u32
        }
    }
}

/// 默认每页条数（REQ-013）。
pub const PAGE_SIZE: u32 = 20;

/// 多条件查询 + 排序分页，并按角色过滤可见范围（REQ-013、REQ-014）。
///
/// 可见范围：买家只能看到已发布商品；卖家看到自己名下全部状态的商品；
/// 管理员可查询全部商品。
pub fn search_items(
    store: &Store,
    session: &Session,
    filter: &ItemFilter,
    sort: SortBy,
    order: SortOrder,
    page: u32,
) -> Result<Page<Item>> {
    if let (Some(min), Some(max)) = (filter.price_min, filter.price_max)
        && min > max
    {
        // 价格区间非法时返回空结果而不是报错（REQ-013）。
        return Ok(Page {
            items: Vec::new(),
            total: 0,
            page: page.max(1),
            page_size: PAGE_SIZE,
        });
    }

    let keyword = filter
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_lowercase);

    let mut matched: Vec<(Item, usize)> = store
        .items
        .iter()
        .filter(|item| match session.role {
            Role::Admin => true,
            Role::Seller => item.seller_id == session.user_id,
            Role::Buyer => item.status.visible_to_buyer(),
        })
        .filter(|item| {
            keyword.as_deref().is_none_or(|keyword| {
                item.title.to_lowercase().contains(keyword)
                    || item.description.to_lowercase().contains(keyword)
            })
        })
        .filter(|item| {
            filter
                .category_id
                .as_deref()
                .is_none_or(|category_id| item.category_id == category_id)
        })
        .filter(|item| filter.status.is_none_or(|status| item.status == status))
        .filter(|item| filter.price_min.is_none_or(|min| item.price >= min))
        .filter(|item| filter.price_max.is_none_or(|max| item.price <= max))
        .map(|item| {
            let sales = store
                .trades
                .iter()
                .filter(|trade| trade.item_id == item.item_id)
                .count();
            (item.clone(), sales)
        })
        .collect();

    matched.sort_by(|left, right| {
        let ordering = match sort {
            SortBy::CreatedAt => left.0.created_at.cmp(&right.0.created_at),
            SortBy::Price => left.0.price.cmp(&right.0.price),
            SortBy::Sales => left.1.cmp(&right.1),
        };
        match order {
            SortOrder::Asc => ordering,
            SortOrder::Desc => ordering.reverse(),
        }
    });

    let total = matched.len();
    let page = page.max(1);
    let start = (page - 1) as usize * PAGE_SIZE as usize;
    let items = matched
        .into_iter()
        .skip(start)
        .take(PAGE_SIZE as usize)
        .map(|(item, _)| item)
        .collect();

    Ok(Page {
        items,
        total,
        page,
        page_size: PAGE_SIZE,
    })
}

/// 卖家统计：在售 / 已售 / 下架数量与预约数量（REQ-024）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SellerStats {
    pub published: usize,
    pub sold: usize,
    pub offline: usize,
    pub reservations: usize,
}

pub fn seller_stats(store: &Store, seller_id: &str) -> Result<SellerStats> {
    let mut stats = SellerStats::default();
    for item in store.items_of_seller(seller_id) {
        match item.status {
            ItemStatus::Published => stats.published += 1,
            ItemStatus::Sold => stats.sold += 1,
            ItemStatus::Offline | ItemStatus::Violation => stats.offline += 1,
            _ => {}
        }
    }
    stats.reservations = store
        .reservations
        .iter()
        .filter(|reservation| reservation.seller_id == seller_id && reservation.status.is_active())
        .count();
    Ok(stats)
}

/// 管理员统计：待审核数、违规数、按分类的已售统计（REQ-024）。
#[derive(Debug, Clone, Default)]
pub struct AdminStats {
    pub pending_items: usize,
    pub violation_items: usize,
    pub sold_by_category: Vec<(String, usize)>,
}

pub fn admin_stats(store: &Store) -> Result<AdminStats> {
    let mut stats = AdminStats {
        pending_items: store
            .items
            .iter()
            .filter(|item| item.status == ItemStatus::Pending)
            .count(),
        violation_items: store
            .items
            .iter()
            .filter(|item| item.status == ItemStatus::Violation)
            .count(),
        sold_by_category: Vec::new(),
    };

    for category in &store.categories {
        let sold = store
            .items
            .iter()
            .filter(|item| {
                item.category_id == category.category_id && item.status == ItemStatus::Sold
            })
            .count();
        if sold > 0 {
            stats.sold_by_category.push((category.name.clone(), sold));
        }
    }
    Ok(stats)
}

// ---------------------------------------------------------------------------
// 列表查询（供界面直接使用）
// ---------------------------------------------------------------------------

/// 我的预约（买家视角）或我收到的预约（卖家视角），时间倒序。
pub fn reservations_of(store: &Store, user_id: &str, as_seller: bool) -> Vec<Reservation> {
    let mut list: Vec<Reservation> = store
        .reservations
        .iter()
        .filter(|reservation| {
            if as_seller {
                reservation.seller_id == user_id
            } else {
                reservation.buyer_id == user_id
            }
        })
        .cloned()
        .collect();
    list.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
    list
}

/// 我买到的 / 我卖出的交易，时间倒序。
pub fn trades_of(store: &Store, user_id: &str, as_seller: bool) -> Vec<Trade> {
    let mut list: Vec<Trade> = store
        .trades
        .iter()
        .filter(|trade| {
            if as_seller {
                trade.seller_id == user_id
            } else {
                trade.buyer_id == user_id
            }
        })
        .cloned()
        .collect();
    list.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
    list
}

/// 待审核商品（管理员）。
pub fn pending_items(store: &Store) -> Vec<Item> {
    store
        .items
        .iter()
        .filter(|item| item.status == ItemStatus::Pending)
        .cloned()
        .collect()
}

/// 已发布商品（管理员做违规处理时的候选对象）。
pub fn published_items(store: &Store) -> Vec<Item> {
    store
        .items
        .iter()
        .filter(|item| item.status == ItemStatus::Published)
        .cloned()
        .collect()
}

/// 未处理完的申诉（管理员）。
pub fn pending_appeals(store: &Store) -> Vec<Appeal> {
    store
        .appeals
        .iter()
        .filter(|appeal| appeal.status == crate::domain::audit::AppealStatus::Pending)
        .cloned()
        .collect()
}

/// 用户列表（管理员用户管理）。
pub fn users(store: &Store) -> Vec<User> {
    let mut list = store.users.clone();
    list.sort_by(|left, right| left.username.cmp(&right.username));
    list
}

/// 分类列表。
pub fn categories(store: &Store) -> Vec<Category> {
    store.categories.clone()
}

/// 某商品的已确认预约数量，用于展示交易状态。
pub fn active_reservations(store: &Store, item_id: &str) -> Vec<Reservation> {
    store
        .reservations
        .iter()
        .filter(|reservation| {
            reservation.item_id == item_id && reservation.status == ReservationStatus::Confirmed
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::user::Session;
    use crate::repo::seed;

    #[test]
    fn buyer_only_sees_published_items() {
        let mut store = seed::demo_store();
        let seller_id = store
            .find_user_by_username("seller")
            .unwrap()
            .user_id
            .clone();
        let buyer_id = store
            .find_user_by_username("buyer")
            .unwrap()
            .user_id
            .clone();

        let draft = crate::service::item::ItemDraft {
            title: "教材".into(),
            description: "".into(),
            category_id: store.categories[0].category_id.clone(),
            price: Some(Decimal::new(10, 0)),
        };
        let _draft_item = crate::service::item::save_draft(&mut store, &seller_id, &draft).unwrap();

        // 演示数据里有 3 件已发布商品，草稿不应出现在买家列表中。
        let buyer_session = Session::new(&buyer_id, Role::Buyer);
        let page = search_items(
            &store,
            &buyer_session,
            &ItemFilter::default(),
            SortBy::CreatedAt,
            SortOrder::Desc,
            1,
        )
        .unwrap();
        assert_eq!(page.total, 3);
        assert!(page.items.iter().all(|item| item.status.visible_to_buyer()));

        let seller_session = Session::new(&seller_id, Role::Seller);
        let page = search_items(
            &store,
            &seller_session,
            &ItemFilter::default(),
            SortBy::CreatedAt,
            SortOrder::Desc,
            1,
        )
        .unwrap();
        // 卖家可见自己名下全部状态：4 件演示商品 + 1 件新建草稿。
        assert_eq!(page.total, 5);
    }

    #[test]
    fn invalid_price_range_returns_empty_page() {
        let store = seed::demo_store();
        let buyer_id = store
            .find_user_by_username("buyer")
            .unwrap()
            .user_id
            .clone();
        let session = Session::new(&buyer_id, Role::Buyer);
        let filter = ItemFilter {
            price_min: Some(Decimal::new(100, 0)),
            price_max: Some(Decimal::new(10, 0)),
            ..Default::default()
        };
        let page =
            search_items(&store, &session, &filter, SortBy::Price, SortOrder::Asc, 1).unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.total, 0);
    }

    #[test]
    fn seller_stats_counts_by_status() {
        let store = seed::demo_store();
        let seller_id = store
            .find_user_by_username("seller")
            .unwrap()
            .user_id
            .clone();
        let stats = seller_stats(&store, &seller_id).unwrap();
        assert_eq!(stats.published, 3);
        assert_eq!(stats.sold, 0);
        assert_eq!(stats.reservations, 0);
        assert_eq!(admin_stats(&store).unwrap().pending_items, 1);
    }
}
