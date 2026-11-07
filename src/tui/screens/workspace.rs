//! 角色工作台：左侧功能菜单 + 右侧内容区 + 底部状态栏。

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph, Wrap};

use crate::app::{App, Dialog, ItemForm, PageData};
use crate::domain::user::Role;
use crate::tui::APP_TITLE;
use crate::tui::screens::{centered, status_style};
use crate::util;

/// 工作台页面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    // 买家
    BrowseItems,
    MyReservations,
    MyTrades,
    // 卖家
    MyItems,
    NewItem,
    SellerReservations,
    SellerTrades,
    SellerStats,
    // 管理员
    AdminAudit,
    AdminViolations,
    AdminAppeals,
    AdminUsers,
    AdminLogs,
    AdminStats,
    // 买家 / 卖家共用
    Notifications,
}

const BUYER_MENU: [Page; 4] = [
    Page::BrowseItems,
    Page::MyReservations,
    Page::MyTrades,
    Page::Notifications,
];

const SELLER_MENU: [Page; 6] = [
    Page::MyItems,
    Page::NewItem,
    Page::SellerReservations,
    Page::SellerTrades,
    Page::SellerStats,
    Page::Notifications,
];

const ADMIN_MENU: [Page; 6] = [
    Page::AdminAudit,
    Page::AdminViolations,
    Page::AdminAppeals,
    Page::AdminUsers,
    Page::AdminLogs,
    Page::AdminStats,
];

impl Page {
    /// 角色对应的菜单（REQ-003：入口按角色过滤）。
    pub fn menu(role: Role) -> &'static [Page] {
        match role {
            Role::Buyer => &BUYER_MENU,
            Role::Seller => &SELLER_MENU,
            Role::Admin => &ADMIN_MENU,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Page::BrowseItems => "浏览商品",
            Page::MyReservations => "我的预约",
            Page::MyTrades => "我的交易",
            Page::MyItems => "我的商品",
            Page::NewItem => "发布商品",
            Page::SellerReservations => "预约管理",
            Page::SellerTrades => "交易管理",
            Page::SellerStats => "数据统计",
            Page::AdminAudit => "商品审核",
            Page::AdminViolations => "违规下架",
            Page::AdminAppeals => "申诉处理",
            Page::AdminUsers => "用户管理",
            Page::AdminLogs => "操作日志",
            Page::AdminStats => "平台统计",
            Page::Notifications => "站内通知",
        }
    }

    /// 该页面承载的需求编号。
    pub fn requirement(self) -> &'static str {
        match self {
            Page::BrowseItems => "REQ-013 / REQ-014 / REQ-015",
            Page::MyReservations => "REQ-016 / REQ-018 / REQ-021",
            Page::MyTrades => "REQ-020",
            Page::MyItems => "REQ-005 / REQ-008 / REQ-014",
            Page::NewItem => "REQ-005 / REQ-009",
            Page::SellerReservations => "REQ-017 / REQ-019",
            Page::SellerTrades => "REQ-020",
            Page::SellerStats => "REQ-024",
            Page::AdminAudit => "REQ-006 / REQ-007 / REQ-008",
            Page::AdminViolations => "REQ-011",
            Page::AdminAppeals => "REQ-011",
            Page::AdminUsers => "REQ-002 / REQ-012",
            Page::AdminLogs => "REQ-025",
            Page::AdminStats => "REQ-024",
            Page::Notifications => "REQ-033",
        }
    }

    /// 页面内的可用操作提示。
    pub fn hint(self) -> &'static str {
        match self {
            Page::BrowseItems => "Enter 预约选中商品",
            Page::MyReservations => "c 取消预约",
            Page::MyTrades => "c 确认交易",
            Page::MyItems => "Enter/e 编辑   s 提交审核   o 下架   p 申诉   n 新建",
            Page::NewItem => "Enter 打开商品表单",
            Page::SellerReservations => "Enter 确认预约   c 取消预约",
            Page::SellerTrades => "c 标记交易完成",
            Page::SellerStats => "",
            Page::AdminAudit => "y 审核通过   n 审核驳回",
            Page::AdminViolations => "v 违规下架",
            Page::AdminAppeals => "y 受理申诉   n 驳回申诉",
            Page::AdminUsers => "b 封禁   u 解封   l 解锁登录",
            Page::AdminLogs => "",
            Page::AdminStats => "",
            Page::Notifications => "r 标记已读   a 全部已读",
        }
    }
}

/// 渲染工作台。
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .split(area);

    render_header(frame, chunks[0], app);

    let body = Layout::horizontal([Constraint::Length(26), Constraint::Min(0)]).split(chunks[1]);
    render_menu(frame, body[0], app);
    render_content(frame, body[1], app);
    render_footer(frame, chunks[2], app);

    if let Some(dialog) = &app.dialog {
        render_dialog(frame, dialog);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered().title(Line::from(Span::styled(
        format!(" {APP_TITLE} "),
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let username = app
        .current_user()
        .map(|user| user.username.clone())
        .unwrap_or_else(|| "-".to_string());
    let role = app.current_role().map(Role::label).unwrap_or("-");

    let info = Line::from(vec![
        Span::styled(
            format!(" 当前账号：{username}"),
            Style::new().fg(Color::White),
        ),
        Span::styled(format!("   角色：{role}"), Style::new().fg(Color::Gray)),
        Span::styled(
            format!("   未读通知：{}", app.unread_notices()),
            Style::new().fg(Color::Yellow),
        ),
    ]);
    frame.render_widget(Paragraph::new(info), inner);
}

fn render_menu(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .menu()
        .iter()
        .enumerate()
        .map(|(index, page)| {
            let selected = index == app.menu_index;
            let style = if selected {
                Style::new()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(Color::Gray)
            };
            ListItem::new(Line::from(format!(" {} ", page.title()))).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::bordered().title(" 功能菜单 "));
    frame.render_widget(list, area);
}

fn render_content(frame: &mut Frame, area: Rect, app: &App) {
    let title = app
        .current_page()
        .map(|page| format!(" {} ", page.title()))
        .unwrap_or_else(|| " - ".to_string());
    let block = Block::bordered().title(Line::from(Span::styled(
        title,
        Style::new().fg(Color::Cyan),
    )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(form) = &app.item_form {
        render_item_form(frame, inner, app, form);
        return;
    }

    let lines = build_lines(app);
    if lines.is_empty() {
        let hint = app
            .current_page()
            .map(|page| page.hint())
            .filter(|hint| !hint.is_empty())
            .unwrap_or("暂无可展示的数据");
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("   {hint}"),
                    Style::new().fg(Color::DarkGray),
                )),
            ]),
            inner,
        );
        return;
    }

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        inner,
    );
}

fn build_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    match &app.data {
        PageData::Items(items) => {
            for (index, item) in items.iter().enumerate() {
                let text = format!(
                    "[{}] {}  ¥{}  卖家：{}",
                    item.status.label(),
                    item.title,
                    item.price,
                    app.username_of(&item.seller_id)
                );
                lines.push(row_line(index == app.cursor, text));
            }
        }
        PageData::Reservations(list) => {
            for (index, reservation) in list.iter().enumerate() {
                let reason = reservation
                    .invalid_reason
                    .as_deref()
                    .map(|reason| format!("（{reason}）"))
                    .unwrap_or_default();
                let text = format!(
                    "{}  商品：{}  买家：{}  状态：{}{}",
                    util::format_time(reservation.created_at),
                    app.item_title_of(&reservation.item_id),
                    app.username_of(&reservation.buyer_id),
                    reservation.status.label(),
                    reason
                );
                lines.push(row_line(index == app.cursor, text));
            }
        }
        PageData::Trades(list) => {
            for (index, trade) in list.iter().enumerate() {
                let state = if trade.buyer_confirmed {
                    "买家已确认"
                } else {
                    "待买家确认"
                };
                let text = format!(
                    "{}  商品：{}  买家：{}  {}  成交时间：{}",
                    trade.trade_id,
                    app.item_title_of(&trade.item_id),
                    app.username_of(&trade.buyer_id),
                    state,
                    util::format_opt_time(trade.completed_at)
                );
                lines.push(row_line(index == app.cursor, text));
            }
        }
        PageData::Notices(list) => {
            for (index, notice) in list.iter().enumerate() {
                let text = format!(
                    "[{}] {}  {}",
                    if notice.read { "已读" } else { "未读" },
                    util::format_time(notice.created_at),
                    notice.message
                );
                let selected = index == app.cursor;
                let style = if selected {
                    Style::new().fg(Color::Black).bg(Color::Cyan)
                } else if notice.read {
                    Style::new().fg(Color::DarkGray)
                } else {
                    Style::new().fg(Color::White)
                };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", marker(selected), text),
                    style,
                )));
            }
        }
        PageData::Logs(list) => {
            for (index, log) in list.iter().enumerate() {
                let text = format!(
                    "{}  {}  {}  {}  {}",
                    util::format_time(log.created_at),
                    app.username_of(&log.operator_id),
                    log.action.label(),
                    log.result.label(),
                    log.detail
                );
                lines.push(row_line(index == app.cursor, text));
            }
        }
        PageData::Appeals(list) => {
            for (index, appeal) in list.iter().enumerate() {
                let text = format!(
                    "{}  卖家：{}  状态：{}  理由：{}",
                    util::format_time(appeal.submitted_at),
                    app.username_of(&appeal.seller_id),
                    appeal.status.label(),
                    appeal.reason
                );
                lines.push(row_line(index == app.cursor, text));
            }
        }
        PageData::Users(list) => {
            for (index, user) in list.iter().enumerate() {
                let roles = user
                    .roles()
                    .iter()
                    .map(|role| role.label())
                    .collect::<Vec<_>>()
                    .join("/");
                let locked = if user.is_locked(util::now()) {
                    "（登录锁定中）"
                } else {
                    ""
                };
                let text = format!(
                    "{:<10} {}  {}  {}{}  {}",
                    user.username,
                    user.phone.clone().unwrap_or_else(|| "-".into()),
                    user.email.clone().unwrap_or_else(|| "-".into()),
                    user.status.label(),
                    locked,
                    roles
                );
                lines.push(row_line(index == app.cursor, text));
            }
        }
        PageData::Lines(content) => {
            for line in content {
                lines.push(Line::from(format!("  {line}")));
            }
        }
        PageData::Empty => {}
    }

    lines
}

fn marker(selected: bool) -> &'static str {
    if selected { "▶ " } else { "  " }
}

fn row_line(selected: bool, text: String) -> Line<'static> {
    let style = if selected {
        Style::new()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    Line::from(Span::styled(format!("{}{text}", marker(selected)), style))
}

fn render_item_form(frame: &mut Frame, area: Rect, app: &App, form: &ItemForm) {
    let categories = &app.store.categories;
    let category = categories
        .get(form.category_index)
        .map(|category| category.name.clone())
        .unwrap_or_else(|| "未选择".to_string());

    let mut lines = Vec::new();
    let fields = [
        ("标题", form.title.clone()),
        ("价格", form.price.clone()),
        ("描述", form.description.clone()),
        ("分类", category),
    ];
    for (index, (label, value)) in fields.iter().enumerate() {
        let focused = index == form.focus;
        let style = if focused {
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray)
        };
        lines.push(Line::from(Span::styled(
            format!(" {label}"),
            Style::new().fg(Color::DarkGray),
        )));
        let cursor = if focused { "▏" } else { "" };
        lines.push(Line::from(Span::styled(
            format!(" {value}{cursor}"),
            style,
        )));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        " Tab 切换字段   分类字段用 ←/→ 选择   Enter 保存   F2 保存并提交审核   Esc 取消",
        Style::new().fg(Color::DarkGray),
    )));

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        area,
    );
}

fn render_dialog(frame: &mut Frame, dialog: &Dialog) {
    let area = centered(frame.area(), 70, 7);
    frame.render_widget(Clear, area);

    let block = Block::bordered().title(Line::from(Span::styled(
        " 操作确认 ",
        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = match dialog {
        Dialog::Input { title, buffer, .. } => vec![
            Line::from(Span::styled(title.clone(), Style::new().fg(Color::White))),
            Line::from(""),
            Line::from(Span::styled(
                format!(" {buffer}▏"),
                Style::new().fg(Color::Cyan),
            )),
            Line::from(Span::styled(
                " Enter 提交   Esc 取消",
                Style::new().fg(Color::DarkGray),
            )),
        ],
        Dialog::Confirm { title, .. } => vec![
            Line::from(Span::styled(title.clone(), Style::new().fg(Color::White))),
            Line::from(""),
            Line::from(Span::styled(
                " y 确认   Esc 取消",
                Style::new().fg(Color::DarkGray),
            )),
        ],
    };
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let page_hint = app
        .current_page()
        .map(|page| page.hint())
        .filter(|hint| !hint.is_empty())
        .unwrap_or("");

    let mut keys = String::from(" Tab 切页   ↑↓ 选择   Enter 操作   Esc 退出登录   F4 切角色   Q 退出 ");
    if !page_hint.is_empty() {
        keys.push_str(" | ");
        keys.push_str(page_hint);
    }

    let lines = vec![
        Line::from(vec![
            Span::styled(" 状态：", Style::new().fg(Color::DarkGray)),
            Span::styled(app.status.text.clone(), status_style(app.status.kind)),
        ]),
        Line::from(Span::styled(keys, Style::new().fg(Color::DarkGray))),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}
