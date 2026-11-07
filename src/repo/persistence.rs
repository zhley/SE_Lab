//! 持久化接口（REQ-022：业务数据保存与关联）。
//!
//! 骨架阶段先定义接口，实现留待数据层完善时补齐。

use std::path::PathBuf;

use crate::error::{AppError, Result};
use crate::repo::Store;

/// 数据持久化能力。
pub trait Persistence {
    /// 从存储介质加载全量数据。
    fn load(&self) -> Result<Store>;
    /// 原子写回全量数据。
    fn save(&self, store: &Store) -> Result<()>;
}

/// JSON 文件持久化（`data/store.json`）。
#[derive(Debug, Clone)]
pub struct JsonFilePersistence {
    pub path: PathBuf,
}

impl JsonFilePersistence {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Persistence for JsonFilePersistence {
    fn load(&self) -> Result<Store> {
        Err(AppError::NotImplemented("repo::persistence::load"))
    }

    fn save(&self, _store: &Store) -> Result<()> {
        Err(AppError::NotImplemented("repo::persistence::save"))
    }
}
