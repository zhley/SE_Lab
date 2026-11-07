//! 商品与审核服务（REQ-004 ~ REQ-011、REQ-014、REQ-024）。

use rust_decimal::Decimal;

use crate::domain::audit::Appeal;
use crate::domain::item::{ALLOWED_TRANSITIONS, Item, ItemStatus};
use crate::error::{AppError, Result};
use crate::repo::Store;
use crate::service::not_implemented;
use crate::util;

/// 新建商品时提交的内容。
#[derive(Debug, Clone, Default)]
pub struct ItemDraft {
    pub title: String,
    pub description: String,
    pub category_id: String,
    pub price: Option<Decimal>,
}

/// 保存草稿（REQ-005）。
pub fn save_draft(_store: &mut Store, _seller_id: &str, _draft: &ItemDraft) -> Result<Item> {
    not_implemented("service::item::save_draft")
}

/// 编辑已有商品；已发布商品修改后回到待审核（REQ-009）。
pub fn update_item(
    _store: &mut Store,
    _seller_id: &str,
    _item_id: &str,
    _draft: &ItemDraft,
    _version: u64,
) -> Result<()> {
    not_implemented("service::item::update_item")
}

/// 提交审核：草稿/审核驳回/已下架 → 待审核（REQ-005、REQ-007、REQ-010）。
pub fn submit_audit(_store: &mut Store, _seller_id: &str, _item_id: &str) -> Result<()> {
    not_implemented("service::item::submit_audit")
}

/// 审核通过：待审核 → 已发布，写入审核记录并通知卖家（REQ-006）。
pub fn approve(_store: &mut Store, _admin_id: &str, _item_id: &str) -> Result<()> {
    not_implemented("service::item::approve")
}

/// 审核驳回：必须填写理由，写入审核记录并通知卖家（REQ-007）。
pub fn reject(_store: &mut Store, _admin_id: &str, _item_id: &str, _reason: &str) -> Result<()> {
    not_implemented("service::item::reject")
}

/// 卖家主动下架：已发布 → 已下架，待确认预约全部失效（REQ-010）。
pub fn take_offline(_store: &mut Store, _seller_id: &str, _item_id: &str) -> Result<()> {
    not_implemented("service::item::take_offline")
}

/// 违规下架：记录违规原因与申诉截止时间，通知卖家（REQ-011）。
pub fn mark_violation(_store: &mut Store, _admin_id: &str, _item_id: &str, _reason: &str) -> Result<()> {
    not_implemented("service::item::mark_violation")
}

/// 卖家提交申诉：7 天内且同一次处理只能申诉一次（REQ-011）。
pub fn submit_appeal(_store: &mut Store, _seller_id: &str, _violation_id: &str, _reason: &str) -> Result<Appeal> {
    not_implemented("service::item::submit_appeal")
}

/// 管理员受理或驳回申诉；受理后商品回到待审核（REQ-011）。
pub fn handle_appeal(
    _store: &mut Store,
    _admin_id: &str,
    _appeal_id: &str,
    _accept: bool,
) -> Result<()> {
    not_implemented("service::item::handle_appeal")
}

/// 状态转换统一入口：校验转换表 + 版本号（REQ-008、REQ-021）。
///
/// 版本号不一致说明用户看到的是旧页面，返回
/// `StateConflict`，界面提示“状态已变化，请刷新”。
pub fn change_status(
    store: &mut Store,
    item_id: &str,
    target: ItemStatus,
    version: u64,
) -> Result<()> {
    let current = store
        .item(item_id)
        .ok_or_else(|| AppError::NotFound("商品不存在或已被删除".into()))?;
    if current.version != version {
        return Err(AppError::StateConflict(
            "商品状态已变化，请刷新后重试".into(),
        ));
    }
    force_status(store, item_id, target)
}

/// 内部状态转换：跳过版本号校验，用于审核、封禁等系统级联操作（REQ-008）。
pub(crate) fn force_status(store: &mut Store, item_id: &str, target: ItemStatus) -> Result<()> {
    let item = store
        .item_mut(item_id)
        .ok_or_else(|| AppError::NotFound("商品不存在或已被删除".into()))?;

    if item.status == target {
        return Ok(());
    }
    if !ALLOWED_TRANSITIONS.contains(&(item.status, target)) {
        return Err(AppError::StateConflict(format!(
            "商品不能从「{}」变更为「{}」",
            item.status.label(),
            target.label()
        )));
    }

    item.status = target;
    item.version += 1;
    item.updated_at = util::now();
    Ok(())
}
