//! 操作日志服务（REQ-025）。

use chrono::{DateTime, Utc};

use crate::domain::EntityKind;
use crate::domain::log::{ActionType, LogResult, OperationLog};
use crate::domain::user::Role;
use crate::error::{AppError, Result};
use crate::repo::Store;

/// 日志查询条件。
#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    pub operator_id: Option<String>,
    pub action: Option<ActionType>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub page: u32,
    pub page_size: u32,
}

/// 待写入的日志条目。
#[derive(Debug, Clone)]
pub struct NewLog<'a> {
    pub operator_id: &'a str,
    pub role: Role,
    pub action: ActionType,
    pub target_type: EntityKind,
    pub target_id: &'a str,
    pub result: LogResult,
    pub detail: &'a str,
}

/// 写入一条操作日志（只增不改，REQ-025）。
pub fn record(store: &mut Store, entry: &NewLog<'_>) -> Result<()> {
    let mut log = OperationLog::new(
        entry.operator_id,
        entry.role,
        entry.action,
        entry.target_type,
        entry.target_id,
        entry.result,
    );
    log.detail = entry.detail.to_string();
    store.logs.push(log);
    Ok(())
}

/// 按条件分页查询日志（时间倒序，仅管理员）。
pub fn query(store: &Store, admin_id: &str, query: &LogQuery) -> Result<Vec<OperationLog>> {
    let operator = store
        .user(admin_id)
        .ok_or_else(|| AppError::NotFound("操作人账号不存在".into()))?;
    if !operator.admin_role {
        return Err(AppError::PermissionDenied(
            "仅管理员可以查询操作日志".into(),
        ));
    }
    if let (Some(from), Some(to)) = (query.from, query.to)
        && from > to
    {
        return Err(AppError::InvalidInput(
            "查询时间段有误：起始时间晚于结束时间".into(),
        ));
    }

    let mut logs: Vec<OperationLog> = store
        .logs
        .iter()
        .filter(|log| {
            query
                .operator_id
                .as_deref()
                .is_none_or(|operator_id| log.operator_id == operator_id)
        })
        .filter(|log| query.action.is_none_or(|action| log.action == action))
        .filter(|log| query.from.is_none_or(|from| log.created_at >= from))
        .filter(|log| query.to.is_none_or(|to| log.created_at <= to))
        .cloned()
        .collect();

    logs.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    if query.page_size == 0 {
        return Ok(logs);
    }
    let start = (query.page.max(1) - 1) as usize * query.page_size as usize;
    Ok(logs
        .into_iter()
        .skip(start)
        .take(query.page_size as usize)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed;

    #[test]
    fn record_appends_only() {
        let mut store = Store::new();
        record(
            &mut store,
            &NewLog {
                operator_id: "U1",
                role: Role::Buyer,
                action: ActionType::CreateReservation,
                target_type: EntityKind::Reservation,
                target_id: "R1",
                result: LogResult::Success,
                detail: "",
            },
        )
        .unwrap();
        assert_eq!(store.logs.len(), 1);
        assert_eq!(store.logs[0].action, ActionType::CreateReservation);
    }

    #[test]
    fn only_admin_can_query() {
        let mut store = seed::demo_store();
        let admin_id = store.find_user_by_username("admin").unwrap().user_id.clone();
        let buyer_id = store.find_user_by_username("buyer").unwrap().user_id.clone();

        record(
            &mut store,
            &NewLog {
                operator_id: &buyer_id,
                role: Role::Buyer,
                action: ActionType::Login,
                target_type: EntityKind::User,
                target_id: &buyer_id,
                result: LogResult::Success,
                detail: "",
            },
        )
        .unwrap();

        let all = query(&store, &admin_id, &LogQuery::default()).unwrap();
        assert_eq!(all.len(), 1);

        let denied = query(&store, &buyer_id, &LogQuery::default());
        assert!(matches!(denied, Err(AppError::PermissionDenied(_))));
    }
}
