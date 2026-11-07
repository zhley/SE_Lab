//! 操作日志模型（REQ-025）。
//!
//! 日志只允许新增，不允许在业务界面修改或删除。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::EntityKind;
use crate::domain::user::Role;
use crate::util;

/// 关键操作类型（REQ-025 列举的操作）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    Register,
    Login,
    LoginFailed,
    SubmitAudit,
    AuditApprove,
    AuditReject,
    TakeOffline,
    Resubmit,
    ViolationOffline,
    AppealSubmit,
    AppealHandle,
    BanUser,
    UnbanUser,
    CreateReservation,
    ConfirmReservation,
    CancelReservation,
    CompleteTrade,
    BuyerConfirm,
}

impl ActionType {
    pub fn label(self) -> &'static str {
        match self {
            ActionType::Register => "注册",
            ActionType::Login => "登录",
            ActionType::LoginFailed => "登录失败",
            ActionType::SubmitAudit => "提交审核",
            ActionType::AuditApprove => "审核通过",
            ActionType::AuditReject => "审核驳回",
            ActionType::TakeOffline => "下架商品",
            ActionType::Resubmit => "重新提交审核",
            ActionType::ViolationOffline => "违规下架",
            ActionType::AppealSubmit => "提交申诉",
            ActionType::AppealHandle => "处理申诉",
            ActionType::BanUser => "封禁用户",
            ActionType::UnbanUser => "解封用户",
            ActionType::CreateReservation => "发起预约",
            ActionType::ConfirmReservation => "确认预约",
            ActionType::CancelReservation => "取消预约",
            ActionType::CompleteTrade => "标记交易完成",
            ActionType::BuyerConfirm => "买家确认交易",
        }
    }
}

/// 操作结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogResult {
    Success,
    Failure,
}

impl LogResult {
    pub fn label(self) -> &'static str {
        match self {
            LogResult::Success => "成功",
            LogResult::Failure => "失败",
        }
    }
}

/// 操作日志条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLog {
    pub log_id: String,
    pub operator_id: String,
    pub role: Role,
    pub action: ActionType,
    pub target_type: EntityKind,
    pub target_id: String,
    pub result: LogResult,
    /// 补充说明，例如失败原因、驳回理由。
    pub detail: String,
    pub created_at: DateTime<Utc>,
}

impl OperationLog {
    pub fn new(
        operator_id: impl Into<String>,
        role: Role,
        action: ActionType,
        target_type: EntityKind,
        target_id: impl Into<String>,
        result: LogResult,
    ) -> Self {
        Self {
            log_id: util::new_id("L"),
            operator_id: operator_id.into(),
            role,
            action,
            target_type,
            target_id: target_id.into(),
            result,
            detail: String::new(),
            created_at: util::now(),
        }
    }
}
