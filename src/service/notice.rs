//! 站内通知服务（REQ-033）。

use crate::domain::EntityKind;
use crate::domain::notice::{NoticeType, Notification};
use crate::error::{AppError, Result};
use crate::repo::Store;

/// 生成一条站内通知。
///
/// 同一接收人 + 同一事件 + 同一业务对象只保留一条，避免重复提交产生重复通知。
pub fn push(
    store: &mut Store,
    user_id: &str,
    event: NoticeType,
    target_type: EntityKind,
    target_id: &str,
    message: &str,
) -> Result<Notification> {
    // 已存在则直接返回，通知属于独立记录，不再重复生成。
    let existing = store
        .notifications
        .iter()
        .find(|notice| {
            notice.user_id == user_id
                && notice.event == event
                && notice.target_type == target_type
                && notice.target_id == target_id
        })
        .cloned();
    if let Some(existing) = existing {
        return Ok(existing);
    }

    let notice = Notification::new(user_id, event, target_type, target_id, message);
    store.notifications.push(notice.clone());
    Ok(notice)
}

/// 列出某用户的全部通知，按时间倒序。
pub fn list(store: &Store, user_id: &str) -> Result<Vec<Notification>> {
    let mut notices: Vec<Notification> = store.notifications_of(user_id).cloned().collect();
    notices.sort_by_key(|notice| std::cmp::Reverse(notice.created_at));
    Ok(notices)
}

/// 标记单条通知为已读。
pub fn mark_read(store: &mut Store, user_id: &str, notice_id: &str) -> Result<()> {
    let notice = store
        .notifications
        .iter_mut()
        .find(|notice| notice.notice_id == notice_id && notice.user_id == user_id)
        .ok_or_else(|| AppError::NotFound("通知不存在".into()))?;
    notice.read = true;
    Ok(())
}

/// 全部标记为已读，返回本次标记的条数。
pub fn mark_all_read(store: &mut Store, user_id: &str) -> Result<usize> {
    let mut count = 0;
    for notice in store
        .notifications
        .iter_mut()
        .filter(|notice| notice.user_id == user_id && !notice.read)
    {
        notice.read = true;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_event_only_creates_one_notice() {
        let mut store = Store::new();
        let first = push(
            &mut store,
            "U1",
            NoticeType::AuditResult,
            EntityKind::Item,
            "I1",
            "审核通过",
        )
        .unwrap();
        let second = push(
            &mut store,
            "U1",
            NoticeType::AuditResult,
            EntityKind::Item,
            "I1",
            "审核通过",
        )
        .unwrap();

        assert_eq!(first.notice_id, second.notice_id);
        assert_eq!(store.notifications.len(), 1);
        assert_eq!(store.unread_count("U1"), 1);
    }

    #[test]
    fn mark_read_reduces_unread_count() {
        let mut store = Store::new();
        let notice = push(
            &mut store,
            "U1",
            NoticeType::TradeCompleted,
            EntityKind::Trade,
            "D1",
            "交易完成",
        )
        .unwrap();
        assert_eq!(store.unread_count("U1"), 1);

        mark_read(&mut store, "U1", &notice.notice_id).unwrap();
        assert_eq!(store.unread_count("U1"), 0);
        // 标记他人通知应被拒绝。
        assert!(mark_read(&mut store, "U2", &notice.notice_id).is_err());

        let other = push(
            &mut store,
            "U1",
            NoticeType::ReservationInvalid,
            EntityKind::Reservation,
            "R1",
            "预约失效",
        )
        .unwrap();
        assert_eq!(mark_all_read(&mut store, "U1").unwrap(), 1);
        assert!(
            store
                .notifications
                .iter()
                .any(|notice| notice.notice_id == other.notice_id && notice.read)
        );
    }
}
