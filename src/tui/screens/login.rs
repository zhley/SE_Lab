//! 登录 / 注册页（REQ-001、REQ-002、REQ-028）。

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::app::{App, AuthForm, AuthMode};
use crate::repo::seed;
use crate::tui::APP_TITLE;
use crate::tui::screens::{centered, status_style};
use crate::util;

const CURSOR: &str = "▏";

/// 渲染登录 / 注册表单。
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).split(area);

    let (height, title, hint) = match app.auth.mode {
        AuthMode::Login => (10, "登录", format!("演示账号：{}", demo_account_hint())),
        AuthMode::Register => (
            19,
            "注册",
            "填写手机号或邮箱后按 F3 获取验证码（演示环境直接显示）".to_string(),
        ),
    };

    let form_area = centered(chunks[0], 64, height);
    let block = Block::bordered().title(Line::from(Span::styled(
        format!(" {APP_TITLE} · {title} "),
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )));

    let mut lines: Vec<Line> = Vec::new();
    for (index, label) in app.auth.labels().iter().enumerate() {
        let focused = index == app.auth.focus;
        lines.push(Line::from(Span::styled(
            format!(" {label}"),
            Style::new().fg(Color::Gray),
        )));

        let raw = app.auth.value(index);
        let shown = if AuthForm::masked(index) {
            util::mask_secret(raw)
        } else {
            raw.to_string()
        };
        let cursor = if focused { CURSOR } else { "" };
        lines.push(Line::from(Span::styled(
            format!(" {shown}{cursor}"),
            input_style(focused),
        )));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        format!(" {hint}"),
        Style::new().fg(Color::DarkGray),
    )));

    let form = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(form, form_area);

    let key_hint = match app.auth.mode {
        AuthMode::Login => " Tab 切换输入框   Enter 登录   F2 去注册   Esc 退出 ",
        AuthMode::Register => {
            " Tab 切换输入框   F3 获取验证码   Enter 注册   F2 返回登录   Esc 返回 "
        }
    };
    let footer = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(" 状态：", Style::new().fg(Color::DarkGray)),
            Span::styled(app.status.text.clone(), status_style(app.status.kind)),
        ]),
        Line::from(Span::styled(key_hint, Style::new().fg(Color::DarkGray))),
    ]);
    frame.render_widget(footer, chunks[1]);
}

fn input_style(focused: bool) -> Style {
    if focused {
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::Gray)
    }
}

/// 演示账号提示（与 `repo::seed` 中的初始化数据保持一致）。
fn demo_account_hint() -> String {
    seed::DEMO_ACCOUNTS
        .iter()
        .map(|(username, password)| format!("{username}/{password}"))
        .collect::<Vec<_>>()
        .join("   ")
}
