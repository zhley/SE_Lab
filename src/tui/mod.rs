//! 表现层：终端初始化、主题与页面渲染入口。
//!
//! 使用 ratatui + crossterm。终端进入 raw mode 与备用屏后，
//! 必须保证异常退出时也能恢复（见 [`install_panic_hook`]）。

pub mod screens;

use std::io::{self, stdout};

use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::{DefaultTerminal, Frame, Terminal, backend::CrosstermBackend};

use crate::app::{App, View};

pub const APP_TITLE: &str = "二手市集";

/// 进入备用屏并开启 raw mode。
pub fn init() -> io::Result<DefaultTerminal> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, Hide)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;
    terminal.clear()?;
    Ok(terminal)
}

/// 恢复终端状态。
pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen, Show)?;
    Ok(())
}

/// panic 时先恢复终端，避免界面残留乱码。
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        default_hook(info);
    }));
}

/// 按当前视图渲染。
pub fn render(frame: &mut Frame, app: &App) {
    match app.view {
        View::Login => screens::login::render(frame, app),
        View::Workspace => screens::workspace::render(frame, app),
    }
}
