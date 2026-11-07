//! 通用工具：时间、编号生成、文本格式化。

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Local, Utc};

/// 进程内自增序号，保证同一毫秒内生成的编号不重复。
static SEQ: AtomicU64 = AtomicU64::new(1);

/// 当前 UTC 时间。
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// 生成业务编号，例如 `I261107120033001`。
///
/// `prefix` 取 `U`/`I`/`R`/`T`/`A`/`V`/`N`/`L` 等实体首字母。
pub fn new_id(prefix: &str) -> String {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let stamp = Local::now().format("%y%m%d%H%M%S");
    format!("{prefix}{stamp}{seq:03}")
}

/// 格式化为本地时间字符串 `2026-11-07 12:30`，用于 TUI 展示。
pub fn format_time(time: DateTime<Utc>) -> String {
    time.with_timezone(&Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

/// 可选时间格式化：`None` 显示为 `-`。
pub fn format_opt_time(time: Option<DateTime<Utc>>) -> String {
    time.map(format_time).unwrap_or_else(|| "-".to_string())
}

/// 生成 `****` 形式的密码占位符，避免密码在界面上回显（REQ-027）。
pub fn mask_secret(secret: &str) -> String {
    "*".repeat(secret.chars().count())
}
