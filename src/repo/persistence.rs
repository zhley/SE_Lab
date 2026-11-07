//! 持久化实现（REQ-022：业务数据保存与关联）。
//!
//! 采用最简单的方案：整个 [`Store`] 序列化成 JSON 文件全量写入。
//! 写入先落到临时文件再替换，避免中途失败损坏原文件。

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

/// JSON 文件持久化（默认 `data/store.json`）。
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
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(AppError::NotFound(format!(
                    "数据文件不存在：{}",
                    self.path.display()
                )));
            }
            Err(err) => {
                return Err(AppError::Io(format!(
                    "读取 {} 失败：{err}",
                    self.path.display()
                )));
            }
        };

        serde_json::from_str(&text).map_err(|err| {
            AppError::Io(format!("数据文件 {} 解析失败：{err}", self.path.display()))
        })
    }

    fn save(&self, store: &Store) -> Result<()> {
        if let Some(dir) = self.path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir)?;
        }

        let text = serde_json::to_string_pretty(store)?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text)?;

        // Windows 下 rename 不能覆盖已存在的文件，先删除旧文件。
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        std::fs::rename(&temp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::user::User;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("se_lab_test_{name}_{}.json", std::process::id()));
        path
    }

    #[test]
    fn save_then_load_roundtrip() {
        let path = temp_path("roundtrip");
        let _ = std::fs::remove_file(&path);

        let persistence = JsonFilePersistence::new(&path);
        let mut store = Store::new();
        store.users.push(User::new("tester", None, Some("t@example.com".into()), "hash"));

        persistence.save(&store).expect("保存失败");
        let loaded = persistence.load().expect("加载失败");
        assert_eq!(loaded.users.len(), 1);
        assert_eq!(loaded.users[0].username, "tester");
        assert_eq!(loaded.users[0].email.as_deref(), Some("t@example.com"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_reports_not_found() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        let persistence = JsonFilePersistence::new(&path);
        assert!(matches!(persistence.load(), Err(AppError::NotFound(_))));
    }
}
