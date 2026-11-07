//! 审核、违规与申诉模型（REQ-006、REQ-007、REQ-011、REQ-025）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::util;

/// 审核结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditResult {
    Approved,
    Rejected,
}

impl AuditResult {
    pub fn label(self) -> &'static str {
        match self {
            AuditResult::Approved => "通过",
            AuditResult::Rejected => "驳回",
        }
    }
}

/// 审核记录：谁在何时对哪个商品给出了什么结论（REQ-006）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub audit_id: String,
    pub item_id: String,
    pub admin_id: String,
    pub result: AuditResult,
    /// 驳回理由，通过时为空（REQ-007：驳回必须填写理由）。
    pub reason: Option<String>,
    pub audited_at: DateTime<Utc>,
}

/// 违规记录（REQ-011）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationRecord {
    pub violation_id: String,
    pub item_id: String,
    pub admin_id: String,
    pub reason: String,
    pub handled_at: DateTime<Utc>,
    /// 申诉截止时间：处理通知后 7 天内可申诉。
    pub appeal_deadline: DateTime<Utc>,
}

impl ViolationRecord {
    /// 是否仍在申诉期内（REQ-011）。
    pub fn can_appeal(&self, now: DateTime<Utc>) -> bool {
        now <= self.appeal_deadline
    }
}

/// 申诉状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppealStatus {
    /// 已提交，待管理员受理。
    Pending,
    /// 已受理：商品回到待审核。
    Accepted,
    /// 已驳回：商品保持违规下架。
    Rejected,
}

impl AppealStatus {
    pub fn label(self) -> &'static str {
        match self {
            AppealStatus::Pending => "待受理",
            AppealStatus::Accepted => "已受理",
            AppealStatus::Rejected => "已驳回",
        }
    }
}

/// 申诉记录（REQ-011：同一次违规处理只能申诉一次）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appeal {
    pub appeal_id: String,
    pub violation_id: String,
    pub seller_id: String,
    pub reason: String,
    pub status: AppealStatus,
    pub submitted_at: DateTime<Utc>,
    pub handled_at: Option<DateTime<Utc>>,
}

impl Appeal {
    pub fn new(violation_id: impl Into<String>, seller_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            appeal_id: util::new_id("P"),
            violation_id: violation_id.into(),
            seller_id: seller_id.into(),
            reason: reason.into(),
            status: AppealStatus::Pending,
            submitted_at: util::now(),
            handled_at: None,
        }
    }
}
