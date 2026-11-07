//! 领域模型层：实体、状态枚举与值对象。
//!
//! 对应需求中的业务对象：用户、商品、预约、交易、审核/违规/申诉、
//! 站内通知、操作日志。模型只描述数据与最基础的不变量，
//! 跨对象的业务规则放在 [`crate::service`] 中。

pub mod audit;
pub mod item;
pub mod log;
pub mod notice;
pub mod reservation;
pub mod trade;
pub mod user;

/// 业务对象类型，用于通知与日志中的 `target_type` 字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EntityKind {
    User,
    Item,
    Reservation,
    Trade,
    Violation,
    Appeal,
}

impl EntityKind {
    pub fn label(self) -> &'static str {
        match self {
            EntityKind::User => "用户",
            EntityKind::Item => "商品",
            EntityKind::Reservation => "预约",
            EntityKind::Trade => "交易",
            EntityKind::Violation => "违规记录",
            EntityKind::Appeal => "申诉",
        }
    }
}
