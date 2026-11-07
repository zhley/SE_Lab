//! 预约与交易服务（REQ-015 ~ REQ-021）。

use crate::domain::reservation::Reservation;
use crate::domain::trade::Trade;
use crate::error::Result;
use crate::repo::Store;
use crate::service::not_implemented;

/// 买家发起预约（REQ-004、REQ-015、REQ-016、REQ-021）。
///
/// 校验：商品状态为已发布、非本人发布、该买家无有效预约、版本一致。
pub fn create_reservation(
    _store: &mut Store,
    _buyer_id: &str,
    _item_id: &str,
    _item_version: u64,
) -> Result<Reservation> {
    not_implemented("service::trade::create_reservation")
}

/// 卖家确认预约（REQ-017）。
///
/// 商品 已发布 → 交易中，本预约置已确认，其余待确认预约失效并通知买家，
/// 同时生成交易记录。
pub fn confirm_reservation(
    _store: &mut Store,
    _seller_id: &str,
    _reservation_id: &str,
    _item_version: u64,
) -> Result<()> {
    not_implemented("service::trade::confirm_reservation")
}

/// 取消预约（REQ-018、REQ-019）。
///
/// 取消已确认的预约后，商品恢复已发布，此前失效的预约在符合条件时恢复为待确认。
pub fn cancel_reservation(_store: &mut Store, _operator_id: &str, _reservation_id: &str) -> Result<()> {
    not_implemented("service::trade::cancel_reservation")
}

/// 卖家标记交易完成：商品 → 已售出，通知买家（REQ-020）。
pub fn complete_trade(_store: &mut Store, _seller_id: &str, _trade_id: &str) -> Result<Trade> {
    not_implemented("service::trade::complete_trade")
}

/// 买家确认交易：仅置“买家已确认”标记（REQ-020）。
pub fn confirm_trade_by_buyer(_store: &mut Store, _buyer_id: &str, _trade_id: &str) -> Result<()> {
    not_implemented("service::trade::confirm_trade_by_buyer")
}

/// 定时任务：超过 7 天未确认的交易自动记为已确认（REQ-020）。
pub fn auto_confirm_overdue(_store: &mut Store) -> Result<usize> {
    not_implemented("service::trade::auto_confirm_overdue")
}

/// 商品下架/违规时，把其下所有待确认预约置为失效（REQ-010、REQ-011）。
pub fn invalidate_pending_reservations(_store: &mut Store, _item_id: &str, _reason: &str) -> Result<usize> {
    not_implemented("service::trade::invalidate_pending_reservations")
}
