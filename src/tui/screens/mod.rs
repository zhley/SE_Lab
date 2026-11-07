//! 页面实现。
//!
//! 骨架阶段只有登录页与工作台外壳；后续按业务模块补充列表页、表单页与确认弹窗。

pub mod login;
pub mod workspace;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};

use crate::app::StatusKind;

/// 状态提示文案的配色。
pub fn status_style(kind: StatusKind) -> Style {
    match kind {
        StatusKind::Info => Style::new().fg(Color::Gray),
        StatusKind::Success => Style::new().fg(Color::Green),
        StatusKind::Error => Style::new().fg(Color::Red),
    }
}

/// 在 `area` 中心取一块 `width x height` 的区域（不超过可用空间）。
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(area);
    let horizontal = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .split(vertical[1]);
    horizontal[1]
}
