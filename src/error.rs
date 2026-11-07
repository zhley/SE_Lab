//! 统一错误类型。
//!
//! 所有业务操作返回 [`Result`]，错误信息直接面向用户展示
//! （对应 REQ-031：失败提示需说明原因与下一步做法）。

use std::fmt;

/// 应用层统一结果类型。
pub type Result<T> = std::result::Result<T, AppError>;

/// 业务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// 骨架阶段的占位：该功能尚未实现，附带模块路径便于定位。
    NotImplemented(&'static str),
    /// 输入校验失败（字段级提示）。
    InvalidInput(String),
    /// 目标对象不存在。
    NotFound(String),
    /// 无权限（REQ-003 / REQ-029）。
    PermissionDenied(String),
    /// 状态已变化或并发冲突（REQ-008 / REQ-021）。
    StateConflict(String),
    /// 唯一约束冲突（REQ-023）。
    Duplicate(String),
    /// IO / 持久化失败。
    Io(String),
}

impl AppError {
    /// 面向用户的提示文案。
    pub fn message(&self) -> String {
        match self {
            AppError::NotImplemented(what) => {
                format!("功能尚未实现（骨架阶段）：{what}")
            }
            AppError::InvalidInput(msg)
            | AppError::NotFound(msg)
            | AppError::PermissionDenied(msg)
            | AppError::StateConflict(msg)
            | AppError::Duplicate(msg)
            | AppError::Io(msg) => msg.clone(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Io(format!("数据序列化失败：{err}"))
    }
}
