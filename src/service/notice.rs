//! 站内通知服务（REQ-033）。

use crate::domain::notice::{NoticeType, Notification};
use crate::error::Result;
use crate::repo::Store;
use crate::service::not_implemented;

/// 生成一条站内通知；同一事件重复提交时不重复生成。
pub fn push(
    _store: &mut Store,
    _user_id: &str,
    _event: NoticeType,
    _target_type: crate::domain::EntityKind,
    _target_id: &str,
    _message: &str,
) -> Result<Notification> {
    not_implemented("service::notice::push")
}

/// 列出某用户的全部通知（按时间倒序）。
pub fn list(_store: &Store, _user_id: &str) -> Result<Vec<Notification>> {
    not_implemented("service::notice::list")
}

/// 标记通知为已读。
pub fn mark_read(_store: &mut Store, _user_id: &str, _notice_id: &str) -> Result<()> {
    not_implemented("service::notice::mark_read")
}

/// 全部标记为已读。
pub fn mark_all_read(_store: &mut Store, _user_id: &str) -> Result<usize> {
    not_implemented("service::notice::mark_all_read")
}
