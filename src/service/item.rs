//! 商品与审核服务（REQ-004 ~ REQ-011、REQ-014）。

use chrono::Duration;
use rust_decimal::Decimal;

use crate::domain::EntityKind;
use crate::domain::audit::{Appeal, AppealStatus, AuditRecord, AuditResult, ViolationRecord};
use crate::domain::item::{ALLOWED_TRANSITIONS, Item, ItemStatus};
use crate::domain::log::{ActionType, LogResult};
use crate::domain::notice::NoticeType;
use crate::domain::user::Role;
use crate::error::{AppError, Result};
use crate::repo::Store;
use crate::service::log::{self, NewLog};
use crate::service::{notice, trade};
use crate::util;

/// 违规申诉期限（REQ-011：7 天内）。
pub const APPEAL_DAYS: i64 = 7;

/// 新建 / 编辑商品时提交的内容。
#[derive(Debug, Clone, Default)]
pub struct ItemDraft {
    pub title: String,
    pub description: String,
    pub category_id: String,
    pub price: Option<Decimal>,
}

/// 保存草稿（REQ-005）。
pub fn save_draft(store: &mut Store, seller_id: &str, draft: &ItemDraft) -> Result<Item> {
    let price = validate_draft(store, draft)?;

    let mut item = Item::new(seller_id, draft.category_id.trim());
    item.title = draft.title.trim().to_string();
    item.description = draft.description.trim().to_string();
    item.price = price;

    store.items.push(item.clone());
    Ok(item)
}

/// 编辑商品（REQ-009）。
///
/// 草稿 / 审核驳回 / 已下架可直接修改；已发布商品修改后回到待审核，
/// 审核期间买家不可见。
pub fn update_item(
    store: &mut Store,
    seller_id: &str,
    item_id: &str,
    draft: &ItemDraft,
    version: u64,
) -> Result<()> {
    let price = validate_draft(store, draft)?;

    let (status, title) = {
        let item = store
            .item(item_id)
            .ok_or_else(|| AppError::NotFound("商品不存在".into()))?;
        if item.seller_id != seller_id {
            return Err(AppError::PermissionDenied(
                "只能修改自己发布的商品".into(),
            ));
        }
        if !item.status.editable() {
            return Err(AppError::StateConflict(format!(
                "「{}」状态的商品不可编辑",
                item.status.label()
            )));
        }
        if item.version != version {
            return Err(AppError::StateConflict(
                "商品状态已变化，请刷新后重试".into(),
            ));
        }
        (item.status, item.title.clone())
    };

    let back_to_audit = status == ItemStatus::Published;
    {
        let item = store
            .item_mut(item_id)
            .ok_or_else(|| AppError::NotFound("商品不存在".into()))?;
        item.title = draft.title.trim().to_string();
        item.description = draft.description.trim().to_string();
        item.category_id = draft.category_id.trim().to_string();
        item.price = price;
        item.updated_at = util::now();
        item.version += 1;
    }

    if back_to_audit {
        force_status(store, item_id, ItemStatus::Pending)?;
        remember_submit(store, seller_id, item_id, &format!("商品「{title}」修改后重新提交审核"))?;
    }

    Ok(())
}

/// 提交审核：草稿 / 审核驳回 / 已下架 → 待审核（REQ-005、REQ-007、REQ-010）。
pub fn submit_audit(store: &mut Store, seller_id: &str, item_id: &str) -> Result<()> {
    let status = {
        let item = store
            .item(item_id)
            .ok_or_else(|| AppError::NotFound("商品不存在".into()))?;
        if item.seller_id != seller_id {
            return Err(AppError::PermissionDenied(
                "只能提交自己发布的商品".into(),
            ));
        }
        validate_draft(
            store,
            &ItemDraft {
                title: item.title.clone(),
                description: item.description.clone(),
                category_id: item.category_id.clone(),
                price: Some(item.price),
            },
        )?;
        item.status
    };

    if !matches!(
        status,
        ItemStatus::Draft | ItemStatus::Rejected | ItemStatus::Offline
    ) {
        return Err(AppError::StateConflict(format!(
            "「{}」状态的商品不能提交审核",
            status.label()
        )));
    }

    force_status(store, item_id, ItemStatus::Pending)?;
    if let Some(item) = store.item_mut(item_id) {
        item.reject_reason = None;
    }
    remember_submit(store, seller_id, item_id, "提交审核")?;
    Ok(())
}

/// 审核通过：待审核 → 已发布，写入审核记录并通知卖家（REQ-006）。
pub fn approve(store: &mut Store, admin_id: &str, item_id: &str) -> Result<()> {
    require_admin(store, admin_id)?;
    let (seller_id, title, status) = item_brief(store, item_id)?;
    if status != ItemStatus::Pending {
        return Err(AppError::StateConflict(format!(
            "商品当前状态为「{}」，不能审核通过",
            status.label()
        )));
    }

    force_status(store, item_id, ItemStatus::Published)?;
    if let Some(item) = store.item_mut(item_id) {
        item.reject_reason = None;
    }

    store.audits.push(AuditRecord {
        audit_id: util::new_id("A"),
        item_id: item_id.to_string(),
        admin_id: admin_id.to_string(),
        result: AuditResult::Approved,
        reason: None,
        audited_at: util::now(),
    });

    notice::push(
        store,
        &seller_id,
        NoticeType::AuditResult,
        EntityKind::Item,
        item_id,
        &format!("商品「{title}」审核通过，已发布"),
    )?;

    log::record(
        store,
        &NewLog {
            operator_id: admin_id,
            role: Role::Admin,
            action: ActionType::AuditApprove,
            target_type: EntityKind::Item,
            target_id: item_id,
            result: LogResult::Success,
            detail: "",
        },
    )?;

    Ok(())
}

/// 审核驳回：必须填写理由，写入审核记录并通知卖家（REQ-007）。
pub fn reject(store: &mut Store, admin_id: &str, item_id: &str, reason: &str) -> Result<()> {
    require_admin(store, admin_id)?;
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::InvalidInput("审核驳回必须填写理由".into()));
    }

    let (seller_id, title, status) = item_brief(store, item_id)?;
    if status != ItemStatus::Pending {
        return Err(AppError::StateConflict(format!(
            "商品当前状态为「{}」，不能驳回",
            status.label()
        )));
    }

    force_status(store, item_id, ItemStatus::Rejected)?;
    if let Some(item) = store.item_mut(item_id) {
        item.reject_reason = Some(reason.to_string());
    }

    store.audits.push(AuditRecord {
        audit_id: util::new_id("A"),
        item_id: item_id.to_string(),
        admin_id: admin_id.to_string(),
        result: AuditResult::Rejected,
        reason: Some(reason.to_string()),
        audited_at: util::now(),
    });

    // 驳回后该商品下待确认预约自动失效（REQ-009）。
    trade::invalidate_pending_reservations(store, item_id, "商品审核驳回，预约自动失效")?;

    notice::push(
        store,
        &seller_id,
        NoticeType::AuditResult,
        EntityKind::Item,
        item_id,
        &format!("商品「{title}」审核驳回：{reason}"),
    )?;

    log::record(
        store,
        &NewLog {
            operator_id: admin_id,
            role: Role::Admin,
            action: ActionType::AuditReject,
            target_type: EntityKind::Item,
            target_id: item_id,
            result: LogResult::Success,
            detail: reason,
        },
    )?;

    Ok(())
}

/// 卖家主动下架：已发布 → 已下架，待确认预约失效（REQ-010）。
pub fn take_offline(store: &mut Store, seller_id: &str, item_id: &str) -> Result<()> {
    let (owner, _, status) = item_brief(store, item_id)?;
    if owner != seller_id {
        return Err(AppError::PermissionDenied(
            "只能下架自己发布的商品".into(),
        ));
    }
    if status != ItemStatus::Published {
        return Err(AppError::StateConflict(format!(
            "「{}」状态的商品不能下架",
            status.label()
        )));
    }

    force_status(store, item_id, ItemStatus::Offline)?;
    trade::invalidate_pending_reservations(store, item_id, "商品已下架，预约自动失效")?;

    log::record(
        store,
        &NewLog {
            operator_id: seller_id,
            role: Role::Seller,
            action: ActionType::TakeOffline,
            target_type: EntityKind::Item,
            target_id: item_id,
            result: LogResult::Success,
            detail: "",
        },
    )?;

    Ok(())
}

/// 违规下架：记录违规原因与申诉截止时间，通知卖家（REQ-011）。
pub fn mark_violation(
    store: &mut Store,
    admin_id: &str,
    item_id: &str,
    reason: &str,
) -> Result<()> {
    require_admin(store, admin_id)?;
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::InvalidInput("违规处理必须填写原因".into()));
    }

    let (seller_id, title, status) = item_brief(store, item_id)?;
    if status == ItemStatus::Sold {
        return Err(AppError::StateConflict("已售出商品不能做违规处理".into()));
    }
    if status != ItemStatus::Published {
        return Err(AppError::StateConflict(format!(
            "「{}」状态的商品不能做违规下架",
            status.label()
        )));
    }

    force_status(store, item_id, ItemStatus::Violation)?;
    trade::invalidate_pending_reservations(store, item_id, "商品违规下架，预约自动失效")?;

    let handled_at = util::now();
    store.violations.push(ViolationRecord {
        violation_id: util::new_id("V"),
        item_id: item_id.to_string(),
        admin_id: admin_id.to_string(),
        reason: reason.to_string(),
        handled_at,
        appeal_deadline: handled_at + Duration::days(APPEAL_DAYS),
    });

    notice::push(
        store,
        &seller_id,
        NoticeType::ViolationHandled,
        EntityKind::Item,
        item_id,
        &format!("商品「{title}」因违规被下架：{reason}（{APPEAL_DAYS} 天内可申诉）"),
    )?;

    log::record(
        store,
        &NewLog {
            operator_id: admin_id,
            role: Role::Admin,
            action: ActionType::ViolationOffline,
            target_type: EntityKind::Item,
            target_id: item_id,
            result: LogResult::Success,
            detail: reason,
        },
    )?;

    Ok(())
}

/// 卖家提交申诉：7 天内且同一次处理只能申诉一次（REQ-011）。
pub fn submit_appeal(
    store: &mut Store,
    seller_id: &str,
    violation_id: &str,
    reason: &str,
) -> Result<Appeal> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::InvalidInput("申诉必须填写理由".into()));
    }

    let violation = store
        .violations
        .iter()
        .find(|record| record.violation_id == violation_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound("违规记录不存在".into()))?;

    let item = store
        .item(&violation.item_id)
        .ok_or_else(|| AppError::NotFound("商品不存在".into()))?;
    if item.seller_id != seller_id {
        return Err(AppError::PermissionDenied("只能申诉自己的商品".into()));
    }

    if store
        .appeals
        .iter()
        .any(|appeal| appeal.violation_id == violation_id)
    {
        return Err(AppError::StateConflict(
            "该违规处理已申诉过，不能重复申诉".into(),
        ));
    }
    if !violation.can_appeal(util::now()) {
        return Err(AppError::StateConflict(
            "已超过 7 天申诉期限".into(),
        ));
    }

    let appeal = Appeal::new(violation_id, seller_id, reason);
    store.appeals.push(appeal.clone());

    log::record(
        store,
        &NewLog {
            operator_id: seller_id,
            role: Role::Seller,
            action: ActionType::AppealSubmit,
            target_type: EntityKind::Appeal,
            target_id: &appeal.appeal_id,
            result: LogResult::Success,
            detail: reason,
        },
    )?;

    Ok(appeal)
}

/// 管理员受理或驳回申诉；受理后商品回到待审核（REQ-011）。
pub fn handle_appeal(
    store: &mut Store,
    admin_id: &str,
    appeal_id: &str,
    accept: bool,
) -> Result<()> {
    require_admin(store, admin_id)?;

    let (seller_id, violation_id) = {
        let appeal = store
            .appeals
            .iter()
            .find(|appeal| appeal.appeal_id == appeal_id)
            .ok_or_else(|| AppError::NotFound("申诉记录不存在".into()))?;
        if appeal.status != AppealStatus::Pending {
            return Err(AppError::StateConflict("该申诉已处理过".into()));
        }
        (appeal.seller_id.clone(), appeal.violation_id.clone())
    };

    let item_id = store
        .violations
        .iter()
        .find(|record| record.violation_id == violation_id)
        .map(|record| record.item_id.clone())
        .ok_or_else(|| AppError::NotFound("违规记录不存在".into()))?;

    if accept {
        force_status(store, &item_id, ItemStatus::Pending)?;
    }

    if let Some(appeal) = store
        .appeals
        .iter_mut()
        .find(|appeal| appeal.appeal_id == appeal_id)
    {
        appeal.status = if accept {
            AppealStatus::Accepted
        } else {
            AppealStatus::Rejected
        };
        appeal.handled_at = Some(util::now());
    }

    notice::push(
        store,
        &seller_id,
        NoticeType::ViolationHandled,
        EntityKind::Item,
        &item_id,
        if accept {
            "申诉已被受理，商品重新进入待审核"
        } else {
            "申诉被驳回，商品保持违规下架"
        },
    )?;

    log::record(
        store,
        &NewLog {
            operator_id: admin_id,
            role: Role::Admin,
            action: ActionType::AppealHandle,
            target_type: EntityKind::Appeal,
            target_id: appeal_id,
            result: LogResult::Success,
            detail: if accept { "受理申诉" } else { "驳回申诉" },
        },
    )?;

    Ok(())
}

/// 状态转换统一入口：校验转换表 + 版本号（REQ-008、REQ-021）。
///
/// 版本号不一致说明用户看到的是旧页面，返回 `StateConflict`，
/// 界面提示“状态已变化，请刷新”。
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

// ---------------------------------------------------------------------------
// 内部辅助
// ---------------------------------------------------------------------------

fn require_admin(store: &Store, admin_id: &str) -> Result<()> {
    let operator = store
        .user(admin_id)
        .ok_or_else(|| AppError::NotFound("操作人账号不存在".into()))?;
    if !operator.admin_role {
        return Err(AppError::PermissionDenied("仅管理员可以执行该操作".into()));
    }
    Ok(())
}

/// 取商品的所有者、标题与当前状态。
fn item_brief(store: &Store, item_id: &str) -> Result<(String, String, ItemStatus)> {
    let item = store
        .item(item_id)
        .ok_or_else(|| AppError::NotFound("商品不存在".into()))?;
    Ok((item.seller_id.clone(), item.title.clone(), item.status))
}

/// 必填项与价格校验（REQ-005：标题、分类、价格缺失或价格非法不允许保存）。
fn validate_draft(store: &Store, draft: &ItemDraft) -> Result<Decimal> {
    let title = draft.title.trim();
    if title.is_empty() {
        return Err(AppError::InvalidInput("商品标题不能为空".into()));
    }
    if title.chars().count() > 40 {
        return Err(AppError::InvalidInput("商品标题不能超过 40 个字符".into()));
    }
    if draft.category_id.trim().is_empty()
        || !store
            .categories
            .iter()
            .any(|category| category.category_id == draft.category_id)
    {
        return Err(AppError::InvalidInput("请选择商品分类".into()));
    }
    let Some(price) = draft.price else {
        return Err(AppError::InvalidInput("请填写商品价格".into()));
    };
    if price < Decimal::ZERO {
        return Err(AppError::InvalidInput("商品价格不能为负数".into()));
    }
    Ok(price)
}

fn remember_submit(store: &mut Store, seller_id: &str, item_id: &str, detail: &str) -> Result<()> {
    log::record(
        store,
        &NewLog {
            operator_id: seller_id,
            role: Role::Seller,
            action: ActionType::SubmitAudit,
            target_type: EntityKind::Item,
            target_id: item_id,
            result: LogResult::Success,
            detail,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed;

    fn draft(store: &Store, title: &str, price: i64) -> ItemDraft {
        ItemDraft {
            title: title.into(),
            description: "九成新".into(),
            category_id: store.categories[0].category_id.clone(),
            price: Some(Decimal::new(price, 0)),
        }
    }

    fn seller_id(store: &Store) -> String {
        store.find_user_by_username("seller").unwrap().user_id.clone()
    }

    fn admin_id(store: &Store) -> String {
        store.find_user_by_username("admin").unwrap().user_id.clone()
    }

    #[test]
    fn draft_must_be_valid_before_saving() {
        let mut store = seed::demo_store();
        let seller = seller_id(&store);
        let mut invalid = draft(&store, "", 10);
        assert!(matches!(
            save_draft(&mut store, &seller, &invalid),
            Err(AppError::InvalidInput(_))
        ));

        invalid.title = "二手教材".into();
        invalid.price = Some(Decimal::new(-1, 0));
        assert!(matches!(
            save_draft(&mut store, &seller, &invalid),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn audit_flow_publishes_item_and_notifies_seller() {
        let mut store = seed::demo_store();
        let seller = seller_id(&store);
        let admin = admin_id(&store);

        let item_draft = draft(&store, "二手教材", 25);
        let item = save_draft(&mut store, &seller, &item_draft).unwrap();
        submit_audit(&mut store, &seller, &item.item_id).unwrap();
        approve(&mut store, &admin, &item.item_id).unwrap();

        let stored = store.item(&item.item_id).unwrap();
        assert_eq!(stored.status, ItemStatus::Published);
        assert_eq!(store.audits.len(), 1);
        assert_eq!(store.unread_count(&seller), 1);
    }

    #[test]
    fn reject_requires_reason_and_hides_item_from_buyers() {
        let mut store = seed::demo_store();
        let seller = seller_id(&store);
        let admin = admin_id(&store);

        let item_draft = draft(&store, "违规物品", 5);
        let item = save_draft(&mut store, &seller, &item_draft).unwrap();
        submit_audit(&mut store, &seller, &item.item_id).unwrap();
        assert!(reject(&mut store, &admin, &item.item_id, "  ").is_err());

        reject(&mut store, &admin, &item.item_id, "描述含违规内容").unwrap();
        let stored = store.item(&item.item_id).unwrap();
        assert_eq!(stored.status, ItemStatus::Rejected);
        assert_eq!(stored.reject_reason.as_deref(), Some("描述含违规内容"));
        assert!(store
            .audits
            .iter()
            .any(|record| record.result == AuditResult::Rejected));
    }

    #[test]
    fn sold_item_cannot_change_status_again() {
        let mut store = seed::demo_store();
        let seller = seller_id(&store);
        let item_draft = draft(&store, "旧书", 8);
        let item = save_draft(&mut store, &seller, &item_draft).unwrap();
        force_status(&mut store, &item.item_id, ItemStatus::Pending).unwrap();
        force_status(&mut store, &item.item_id, ItemStatus::Published).unwrap();
        force_status(&mut store, &item.item_id, ItemStatus::InTrade).unwrap();
        force_status(&mut store, &item.item_id, ItemStatus::Sold).unwrap();

        let version = store.item(&item.item_id).unwrap().version;
        assert!(matches!(
            change_status(&mut store, &item.item_id, ItemStatus::Published, version),
            Err(AppError::StateConflict(_))
        ));
    }
}
