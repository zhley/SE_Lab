//! 站内通知模型（REQ-033）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::EntityKind;
use crate::util;

/// 通知事件类型：需求中约定的六类事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoticeType {
    /// 预约被确认。
    ReservationConfirmed,
    /// 预约失效（被其他预约挤出、商品下架等）。
    ReservationInvalid,
    /// 预约被取消。
    ReservationCanceled,
    /// 交易完成。
    TradeCompleted,
    /// 商品审核结果（通过 / 驳回）。
    AuditResult,
    /// 违规下架处理结果（含申诉结果）。
    ViolationHandled,
}

impl NoticeType {
    pub fn label(self) -> &'static str {
        match self {
            NoticeType::ReservationConfirmed => "预约被确认",
            NoticeType::ReservationInvalid => "预约失效",
            NoticeType::ReservationCanceled => "预约被取消",
            NoticeType::TradeCompleted => "交易完成",
            NoticeType::AuditResult => "商品审核结果",
            NoticeType::ViolationHandled => "违规下架处理结果",
        }
    }
}

/// 站内通知。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub notice_id: String,
    /// 接收人。
    pub user_id: String,
    pub event: NoticeType,
    pub target_type: EntityKind,
    pub target_id: String,
    /// 通知正文，直接展示给用户。
    pub message: String,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

impl Notification {
    pub fn new(
        user_id: impl Into<String>,
        event: NoticeType,
        target_type: EntityKind,
        target_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            notice_id: util::new_id("N"),
            user_id: user_id.into(),
            event,
            target_type,
            target_id: target_id.into(),
            message: message.into(),
            read: false,
            created_at: util::now(),
        }
    }
}
