//! 登录页（REQ-001、REQ-002）。

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::app::{App, LoginField};
use crate::tui::APP_TITLE;
use crate::tui::screens::{centered, status_style};
use crate::util;

const CURSOR: &str = "▏";

/// 渲染登录表单。
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).split(area);

    let form_area = centered(chunks[0], 62, 13);
    let block = Block::bordered().title(Line::from(Span::styled(
        format!(" {APP_TITLE} · 登录 "),
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )));

    let account_focused = app.login.field == LoginField::Account;
    let password_focused = app.login.field == LoginField::Password;

    let account_line = format!(
        " {}{}",
        app.login.account,
        if account_focused { CURSOR } else { "" }
    );
    let password_line = format!(
        " {}{}",
        util::mask_secret(&app.login.password),
        if password_focused { CURSOR } else { "" }
    );

    let lines = vec![
        Line::from(Span::styled(
            "账号（用户名 / 手机号 / 邮箱）",
            Style::new().fg(Color::Gray),
        )),
        Line::from(Span::styled(account_line, input_style(account_focused))),
        Line::from(""),
        Line::from(Span::styled("密码", Style::new().fg(Color::Gray))),
        Line::from(Span::styled(password_line, input_style(password_focused))),
        Line::from(""),
        Line::from(Span::styled(
            " 密码不会明文回显（REQ-027）；连续失败 5 次锁定 15 分钟（REQ-002）",
            Style::new().fg(Color::DarkGray),
        )),
    ];

    let form = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(form, form_area);

    let footer = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(" 状态：", Style::new().fg(Color::DarkGray)),
            Span::styled(app.status.text.clone(), status_style(app.status.kind)),
        ]),
        Line::from(Span::styled(
            " Tab 切换输入框   Enter 登录   F1/F2/F3 演示角色（临时）   Esc 退出 ",
            Style::new().fg(Color::DarkGray),
        )),
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
