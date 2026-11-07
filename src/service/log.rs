//! 操作日志服务（REQ-025）。

use chrono::{DateTime, Utc};

use crate::domain::EntityKind;
use crate::domain::log::{ActionType, LogResult, OperationLog};
use crate::domain::user::Role;
use crate::error::Result;
use crate::repo::Store;
use crate::service::not_implemented;

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

/// 写入一条操作日志。
pub fn record(_store: &mut Store, _entry: &NewLog<'_>) -> Result<()> {
    not_implemented("service::log::record")
}

/// 按条件分页查询日志（时间倒序，仅管理员）。
pub fn query(_store: &Store, _admin_id: &str, _query: &LogQuery) -> Result<Vec<OperationLog>> {
    not_implemented("service::log::query")
}
