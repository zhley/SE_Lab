//! 预约与交易服务（REQ-015 ~ REQ-021）。

use chrono::Duration;

use crate::domain::EntityKind;
use crate::domain::item::ItemStatus;
use crate::domain::log::{ActionType, LogResult};
use crate::domain::notice::NoticeType;
use crate::domain::reservation::{Reservation, ReservationStatus};
use crate::domain::trade::Trade;
use crate::domain::user::Role;
use crate::error::{AppError, Result};
use crate::repo::Store;
use crate::service::item::force_status;
use crate::service::log::{self, NewLog};
use crate::service::notice;
use crate::util;

/// 因其他预约成交而失效的原因标记，取消后据此恢复（REQ-019）。
const CONFLICT_REASON: &str = "商品已被其他预约成交，预约暂时失效";
/// 买家确认截止天数（REQ-020）。
pub const CONFIRM_DAYS: i64 = 7;

/// 买家发起预约（REQ-004、REQ-015、REQ-016、REQ-021）。
pub fn create_reservation(
    store: &mut Store,
    buyer_id: &str,
    item_id: &str,
    item_version: u64,
) -> Result<Reservation> {
    let (seller_id, status, version) = {
        let item = store
            .item(item_id)
            .ok_or_else(|| AppError::NotFound("商品不存在或已失效".into()))?;
        (item.seller_id.clone(), item.status, item.version)
    };

    if status != ItemStatus::Published {
        return Err(AppError::StateConflict(format!(
            "商品当前状态为「{}」，无法预约，请刷新后重试",
            status.label()
        )));
    }
    if seller_id == buyer_id {
        return Err(AppError::PermissionDenied("不能预约自己发布的商品".into()));
    }
    if version != item_version {
        return Err(AppError::StateConflict(
            "商品状态已变化，请刷新后重试".into(),
        ));
    }
    if store.active_reservation(item_id, buyer_id).is_some() {
        return Err(AppError::StateConflict("已预约该商品".into()));
    }

    let reservation = Reservation::new(item_id, buyer_id, &seller_id, item_version);
    store.reservations.push(reservation.clone());

    log::record(
        store,
        &NewLog {
            operator_id: buyer_id,
            role: Role::Buyer,
            action: ActionType::CreateReservation,
            target_type: EntityKind::Reservation,
            target_id: &reservation.reservation_id,
            result: LogResult::Success,
            detail: "",
        },
    )?;

    Ok(reservation)
}

/// 卖家确认预约（REQ-017）：商品进入交易中，其余待确认预约失效。
pub fn confirm_reservation(
    store: &mut Store,
    seller_id: &str,
    reservation_id: &str,
    item_version: u64,
) -> Result<()> {
    let (item_id, buyer_id, status) = {
        let reservation = store
            .reservation(reservation_id)
            .ok_or_else(|| AppError::NotFound("预约不存在".into()))?;
        (
            reservation.item_id.clone(),
            reservation.buyer_id.clone(),
            reservation.status,
        )
    };
    if !status.is_active() {
        return Err(AppError::StateConflict(format!(
            "预约{}，无法确认",
            status.label()
        )));
    }
    if status == ReservationStatus::Confirmed {
        return Err(AppError::StateConflict("该预约已确认，无需重复操作".into()));
    }

    let (owner, item_status, version) = {
        let item = store
            .item(&item_id)
            .ok_or_else(|| AppError::NotFound("商品不存在".into()))?;
        (item.seller_id.clone(), item.status, item.version)
    };
    if owner != seller_id {
        return Err(AppError::PermissionDenied(
            "只能确认自己商品下的预约".into(),
        ));
    }
    if item_status != ItemStatus::Published {
        return Err(AppError::StateConflict(format!(
            "商品当前状态为「{}」，无法确认预约",
            item_status.label()
        )));
    }
    if version != item_version {
        return Err(AppError::StateConflict(
            "商品状态已变化，请刷新后重试".into(),
        ));
    }

    force_status(store, &item_id, ItemStatus::InTrade)?;

    if let Some(reservation) = store.reservation_mut(reservation_id) {
        reservation.status = ReservationStatus::Confirmed;
        reservation.updated_at = util::now();
    }

    // 其余待确认预约自动失效（REQ-017、REQ-033）。
    let others: Vec<(String, String)> = store
        .reservations
        .iter()
        .filter(|other| {
            other.item_id == item_id
                && other.reservation_id != reservation_id
                && other.status == ReservationStatus::Pending
        })
        .map(|other| (other.reservation_id.clone(), other.buyer_id.clone()))
        .collect();

    for (other_id, other_buyer) in others {
        if let Some(other) = store.reservation_mut(&other_id) {
            other.status = ReservationStatus::Invalid;
            other.invalid_reason = Some(CONFLICT_REASON.to_string());
            other.updated_at = util::now();
        }
        notice::push(
            store,
            &other_buyer,
            NoticeType::ReservationInvalid,
            EntityKind::Reservation,
            &other_id,
            "卖家已确认其他买家的预约，您的预约自动失效",
        )?;
    }

    // 生成交易记录（状态为已完成，等待卖家标记完成时间与买家确认）。
    let trade = Trade::new(reservation_id, &item_id, &buyer_id, seller_id);
    store.trades.push(trade);

    notice::push(
        store,
        &buyer_id,
        NoticeType::ReservationConfirmed,
        EntityKind::Reservation,
        reservation_id,
        "卖家已确认您的预约，商品进入交易中",
    )?;

    log::record(
        store,
        &NewLog {
            operator_id: seller_id,
            role: Role::Seller,
            action: ActionType::ConfirmReservation,
            target_type: EntityKind::Reservation,
            target_id: reservation_id,
            result: LogResult::Success,
            detail: "",
        },
    )?;

    Ok(())
}

/// 取消预约（REQ-018、REQ-019）。
pub fn cancel_reservation(
    store: &mut Store,
    operator_id: &str,
    reservation_id: &str,
) -> Result<()> {
    let (item_id, buyer_id, seller_id, status) = {
        let reservation = store
            .reservation(reservation_id)
            .ok_or_else(|| AppError::NotFound("预约不存在".into()))?;
        (
            reservation.item_id.clone(),
            reservation.buyer_id.clone(),
            reservation.seller_id.clone(),
            reservation.status,
        )
    };

    if operator_id != buyer_id && operator_id != seller_id {
        return Err(AppError::PermissionDenied("只能取消自己的预约".into()));
    }
    if !status.is_active() {
        return Err(AppError::StateConflict(format!(
            "预约{}，无需取消",
            status.label()
        )));
    }

    let item_status = store
        .item(&item_id)
        .map(|item| item.status)
        .ok_or_else(|| AppError::NotFound("商品不存在".into()))?;
    if item_status == ItemStatus::Sold {
        return Err(AppError::StateConflict("商品已售出，不能取消预约".into()));
    }

    let confirmed = status == ReservationStatus::Confirmed;

    if let Some(reservation) = store.reservation_mut(reservation_id) {
        reservation.status = ReservationStatus::Canceled;
        reservation.invalid_reason = None;
        reservation.updated_at = util::now();
    }

    // 通知交易的另一方（REQ-033）。
    let receiver = if operator_id == buyer_id {
        &seller_id
    } else {
        &buyer_id
    };
    notice::push(
        store,
        receiver,
        NoticeType::ReservationCanceled,
        EntityKind::Reservation,
        reservation_id,
        "关联的预约已被取消",
    )?;

    // 已确认预约取消后商品恢复已发布，被挤掉的预约恢复为待确认（REQ-019）。
    if confirmed && item_status == ItemStatus::InTrade {
        force_status(store, &item_id, ItemStatus::Published)?;
        let now = util::now();
        for reservation in store.reservations.iter_mut().filter(|reservation| {
            reservation.item_id == item_id
                && reservation.status == ReservationStatus::Invalid
                && reservation.invalid_reason.as_deref() == Some(CONFLICT_REASON)
        }) {
            reservation.status = ReservationStatus::Pending;
            reservation.invalid_reason = None;
            reservation.updated_at = now;
        }
    }

    log::record(
        store,
        &NewLog {
            operator_id,
            role: if operator_id == buyer_id {
                Role::Buyer
            } else {
                Role::Seller
            },
            action: ActionType::CancelReservation,
            target_type: EntityKind::Reservation,
            target_id: reservation_id,
            result: LogResult::Success,
            detail: "",
        },
    )?;

    Ok(())
}

/// 卖家标记交易完成：商品 → 已售出，通知买家（REQ-020）。
pub fn complete_trade(store: &mut Store, seller_id: &str, trade_id: &str) -> Result<Trade> {
    let (item_id, buyer_id, trade_seller) = {
        let trade = store
            .trade(trade_id)
            .ok_or_else(|| AppError::NotFound("交易记录不存在".into()))?;
        (
            trade.item_id.clone(),
            trade.buyer_id.clone(),
            trade.seller_id.clone(),
        )
    };
    if trade_seller != seller_id {
        return Err(AppError::PermissionDenied(
            "只有卖家可以标记交易完成".into(),
        ));
    }

    let item_status = store
        .item(&item_id)
        .map(|item| item.status)
        .ok_or_else(|| AppError::NotFound("商品不存在".into()))?;
    if item_status != ItemStatus::InTrade {
        return Err(AppError::StateConflict(format!(
            "商品当前状态为「{}」，无法标记交易完成",
            item_status.label()
        )));
    }

    force_status(store, &item_id, ItemStatus::Sold)?;

    let completed_at = util::now();
    let updated = {
        let trade = store
            .trades
            .iter_mut()
            .find(|trade| trade.trade_id == trade_id)
            .ok_or_else(|| AppError::NotFound("交易记录不存在".into()))?;
        trade.completed_at = Some(completed_at);
        trade.confirm_deadline = Some(completed_at + Duration::days(CONFIRM_DAYS));
        trade.clone()
    };

    notice::push(
        store,
        &buyer_id,
        NoticeType::TradeCompleted,
        EntityKind::Trade,
        trade_id,
        "卖家已标记交易完成，请确认交易",
    )?;

    log::record(
        store,
        &NewLog {
            operator_id: seller_id,
            role: Role::Seller,
            action: ActionType::CompleteTrade,
            target_type: EntityKind::Trade,
            target_id: trade_id,
            result: LogResult::Success,
            detail: "",
        },
    )?;

    Ok(updated)
}

/// 买家确认交易：仅置“买家已确认”标记（REQ-020）。
pub fn confirm_trade_by_buyer(store: &mut Store, buyer_id: &str, trade_id: &str) -> Result<()> {
    let trade = store
        .trades
        .iter_mut()
        .find(|trade| trade.trade_id == trade_id)
        .ok_or_else(|| AppError::NotFound("交易记录不存在".into()))?;

    if trade.buyer_id != buyer_id {
        return Err(AppError::PermissionDenied("只有买家可以确认交易".into()));
    }
    // 重复点击只记录一次。
    if trade.buyer_confirmed {
        return Ok(());
    }
    trade.buyer_confirmed = true;

    log::record(
        store,
        &NewLog {
            operator_id: buyer_id,
            role: Role::Buyer,
            action: ActionType::BuyerConfirm,
            target_type: EntityKind::Trade,
            target_id: trade_id,
            result: LogResult::Success,
            detail: "",
        },
    )?;

    Ok(())
}

/// 定时任务：超过 7 天未确认的交易自动记为已确认（REQ-020）。
pub fn auto_confirm_overdue(store: &mut Store) -> Result<usize> {
    let now = util::now();
    let mut count = 0;
    for trade in store.trades.iter_mut() {
        if !trade.buyer_confirmed
            && trade
                .confirm_deadline
                .is_some_and(|deadline| deadline <= now)
        {
            trade.buyer_confirmed = true;
            count += 1;
        }
    }
    Ok(count)
}

/// 商品下架 / 违规 / 驳回时，把其下所有待确认预约置为失效并通知买家
/// （REQ-010、REQ-011）。返回失效条数。
pub fn invalidate_pending_reservations(
    store: &mut Store,
    item_id: &str,
    reason: &str,
) -> Result<usize> {
    let now = util::now();
    let mut affected: Vec<(String, String)> = Vec::new();

    for reservation in store.reservations.iter_mut().filter(|reservation| {
        reservation.item_id == item_id && reservation.status == ReservationStatus::Pending
    }) {
        reservation.status = ReservationStatus::Invalid;
        reservation.invalid_reason = Some(reason.to_string());
        reservation.updated_at = now;
        affected.push((
            reservation.reservation_id.clone(),
            reservation.buyer_id.clone(),
        ));
    }

    let count = affected.len();
    for (reservation_id, buyer_id) in affected {
        notice::push(
            store,
            &buyer_id,
            NoticeType::ReservationInvalid,
            EntityKind::Reservation,
            &reservation_id,
            reason,
        )?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed;
    use crate::service::item;

    fn ids(store: &Store) -> (String, String, String) {
        (
            store
                .find_user_by_username("seller")
                .unwrap()
                .user_id
                .clone(),
            store
                .find_user_by_username("buyer")
                .unwrap()
                .user_id
                .clone(),
            store
                .find_user_by_username("admin")
                .unwrap()
                .user_id
                .clone(),
        )
    }

    fn published_item(store: &mut Store, seller: &str) -> String {
        let draft = item::ItemDraft {
            title: "二手显示器".into(),
            description: "自用一年".into(),
            category_id: store.categories[1].category_id.clone(),
            price: Some(rust_decimal::Decimal::new(300, 0)),
        };
        let created = item::save_draft(store, seller, &draft).unwrap();
        item::submit_audit(store, seller, &created.item_id).unwrap();
        let (_, _, admin) = ids(store);
        item::approve(store, &admin, &created.item_id).unwrap();
        created.item_id
    }

    #[test]
    fn cannot_reserve_own_item_or_duplicate() {
        let mut store = seed::demo_store();
        let (seller, buyer, _) = ids(&store);
        let item_id = published_item(&mut store, &seller);
        let version = store.item(&item_id).unwrap().version;

        assert!(create_reservation(&mut store, &seller, &item_id, version).is_err());

        create_reservation(&mut store, &buyer, &item_id, version).unwrap();
        // 重复预约被拦截（REQ-016）。
        assert!(matches!(
            create_reservation(&mut store, &buyer, &item_id, version),
            Err(AppError::StateConflict(_))
        ));
        // 旧页面版本号被拒绝（REQ-021）。
        assert!(matches!(
            create_reservation(&mut store, &buyer, &item_id, version - 1),
            Err(AppError::StateConflict(_))
        ));
    }

    #[test]
    fn confirm_invalidates_others_and_creates_trade() {
        let mut store = seed::demo_store();
        let (seller, buyer, _) = ids(&store);
        let item_id = published_item(&mut store, &seller);
        let version = store.item(&item_id).unwrap().version;

        let second_buyer = crate::domain::user::User::new("buyer2", None, None, "x");
        let second_buyer_id = second_buyer.user_id.clone();
        store.users.push(second_buyer);

        let first = create_reservation(&mut store, &buyer, &item_id, version).unwrap();
        let second = create_reservation(&mut store, &second_buyer_id, &item_id, version).unwrap();

        confirm_reservation(&mut store, &seller, &first.reservation_id, version).unwrap();

        assert_eq!(store.item(&item_id).unwrap().status, ItemStatus::InTrade);
        assert_eq!(
            store.reservation(&first.reservation_id).unwrap().status,
            ReservationStatus::Confirmed
        );
        assert_eq!(
            store.reservation(&second.reservation_id).unwrap().status,
            ReservationStatus::Invalid
        );
        assert_eq!(store.trades.len(), 1);
    }

    #[test]
    fn cancel_confirmed_reservation_restores_item_and_other_reservations() {
        let mut store = seed::demo_store();
        let (seller, buyer, _) = ids(&store);
        let item_id = published_item(&mut store, &seller);
        let version = store.item(&item_id).unwrap().version;

        let second_buyer = crate::domain::user::User::new("buyer3", None, None, "x");
        let second_buyer_id = second_buyer.user_id.clone();
        store.users.push(second_buyer);

        let first = create_reservation(&mut store, &buyer, &item_id, version).unwrap();
        let second = create_reservation(&mut store, &second_buyer_id, &item_id, version).unwrap();
        confirm_reservation(&mut store, &seller, &first.reservation_id, version).unwrap();

        cancel_reservation(&mut store, &buyer, &first.reservation_id).unwrap();

        assert_eq!(store.item(&item_id).unwrap().status, ItemStatus::Published);
        assert_eq!(
            store.reservation(&second.reservation_id).unwrap().status,
            ReservationStatus::Pending
        );
    }

    #[test]
    fn complete_trade_marks_item_sold_and_notifies_buyer() {
        let mut store = seed::demo_store();
        let (seller, buyer, _) = ids(&store);
        let item_id = published_item(&mut store, &seller);
        let version = store.item(&item_id).unwrap().version;

        let reservation = create_reservation(&mut store, &buyer, &item_id, version).unwrap();
        confirm_reservation(&mut store, &seller, &reservation.reservation_id, version).unwrap();
        let trade_id = store.trades[0].trade_id.clone();

        complete_trade(&mut store, &seller, &trade_id).unwrap();
        assert_eq!(store.item(&item_id).unwrap().status, ItemStatus::Sold);
        assert!(store.unread_count(&buyer) >= 1);

        confirm_trade_by_buyer(&mut store, &buyer, &trade_id).unwrap();
        assert!(store.trade(&trade_id).unwrap().buyer_confirmed);
    }
}
