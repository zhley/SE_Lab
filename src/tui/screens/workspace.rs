//! 角色工作台：左侧功能菜单 + 右侧内容区 + 底部状态栏。
//!
//! 骨架阶段内容区展示该页面「对应需求」与「计划实现」，后续替换为真正的列表/表单。

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph, Wrap};

use crate::app::App;
use crate::domain::user::Role;
use crate::tui::APP_TITLE;
use crate::tui::screens::status_style;

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

const ADMIN_MENU: [Page; 5] = [
    Page::AdminAudit,
    Page::AdminViolations,
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
            Page::AdminViolations => "违规与申诉",
            Page::AdminUsers => "用户管理",
            Page::AdminLogs => "操作日志",
            Page::AdminStats => "平台统计",
            Page::Notifications => "站内通知",
        }
    }

    /// 该页面承载的需求编号。
    pub fn requirement(self) -> &'static str {
        match self {
            Page::BrowseItems => "REQ-013 / REQ-014 / REQ-026",
            Page::MyReservations => "REQ-016 / REQ-018 / REQ-021",
            Page::MyTrades => "REQ-020",
            Page::MyItems => "REQ-005 / REQ-008 / REQ-014",
            Page::NewItem => "REQ-005 / REQ-009",
            Page::SellerReservations => "REQ-017 / REQ-019",
            Page::SellerTrades => "REQ-020",
            Page::SellerStats => "REQ-024",
            Page::AdminAudit => "REQ-006 / REQ-007 / REQ-008",
            Page::AdminViolations => "REQ-011",
            Page::AdminUsers => "REQ-002 / REQ-012",
            Page::AdminLogs => "REQ-025",
            Page::AdminStats => "REQ-024",
            Page::Notifications => "REQ-033",
        }
    }

    /// 计划实现的要点，兼作开发清单。
    pub fn todos(self) -> &'static [&'static str] {
        match self {
            Page::BrowseItems => &[
                "关键词 / 分类 / 价格区间 / 状态组合查询",
                "按发布时间、价格、销量排序并分页",
                "仅展示已发布商品（买家可见范围）",
                "进入详情页发起预约",
            ],
            Page::MyReservations => &[
                "查看本人预约及当前状态",
                "取消待确认 / 已确认预约",
                "提交前校验商品与预约状态（版本冲突提示刷新）",
            ],
            Page::MyTrades => &[
                "查看本人交易记录",
                "买家确认交易（7 天未确认自动确认）",
            ],
            Page::MyItems => &[
                "按状态筛选本人商品（草稿/待审核/已发布/已售出…）",
                "编辑草稿与审核驳回商品",
                "提交审核、主动下架",
            ],
            Page::NewItem => &[
                "填写标题、分类、价格、描述并保存草稿",
                "提交审核（重复提交只生成一条待审核记录）",
                "表单校验：必填项、价格非负",
            ],
            Page::SellerReservations => &[
                "查看名下商品的待确认预约",
                "确认预约：商品进入交易中，其余预约失效",
                "不可逆操作前二次确认并说明影响",
            ],
            Page::SellerTrades => &[
                "标记交易完成：商品转为已售出",
                "生成交易完成通知给买家",
            ],
            Page::SellerStats => &[
                "在售 / 已售 / 下架商品数量",
                "预约数量统计",
            ],
            Page::AdminAudit => &[
                "待审核商品列表",
                "审核通过 / 驳回（驳回必填理由）",
                "写入审核记录并通知卖家",
            ],
            Page::AdminViolations => &[
                "违规下架并记录原因与申诉截止时间",
                "7 天内申诉受理，受理后商品回到待审核",
                "同一次处理只能申诉一次",
            ],
            Page::AdminUsers => &[
                "封禁 / 解封用户（必填理由并记日志）",
                "手动解锁登录锁定",
                "封禁时下架该用户已发布商品",
            ],
            Page::AdminLogs => &[
                "按操作人 / 操作类型 / 时间段查询",
                "时间倒序分页，日志只读",
            ],
            Page::AdminStats => &[
                "待审核商品数、违规商品数",
                "按分类统计已售商品数",
            ],
            Page::Notifications => &[
                "查看未读 / 已读通知",
                "标记已读后顶栏未读数同步减少",
            ],
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

    let body = Layout::horizontal([Constraint::Length(30), Constraint::Min(0)]).split(chunks[1]);
    render_menu(frame, body[0], app);
    render_content(frame, body[1], app);
    render_footer(frame, chunks[2], app);
}

fn render_header(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
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
        Span::styled(format!(" 当前账号：{username}"), Style::new().fg(Color::White)),
        Span::styled(format!("   角色：{role}"), Style::new().fg(Color::Gray)),
        Span::styled(
            format!("   未读通知：{}", app.unread_notices()),
            Style::new().fg(Color::Yellow),
        ),
    ]);
    frame.render_widget(Paragraph::new(info), inner);
}

fn render_menu(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
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

fn render_content(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let title = app
        .current_page()
        .map(|page| format!(" {} ", page.title()))
        .unwrap_or_else(|| " - ".to_string());
    let block = Block::bordered().title(Line::from(Span::styled(
        title,
        Style::new().fg(Color::Cyan),
    )));

    let mut lines: Vec<Line> = Vec::new();
    match app.current_page() {
        Some(page) => {
            lines.push(Line::from(vec![
                Span::styled("对应需求：", Style::new().fg(Color::Gray)),
                Span::styled(page.requirement(), Style::new().fg(Color::Cyan)),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "计划实现",
                Style::new().add_modifier(Modifier::BOLD),
            )));
            for todo in page.todos() {
                lines.push(Line::from(format!("  · {todo}")));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "当前状态：骨架页面，业务逻辑待实现",
                Style::new().fg(Color::Yellow),
            )));
        }
        None => lines.push(Line::from("请选择左侧菜单")),
    }

    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn render_footer(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let lines = vec![
        Line::from(vec![
            Span::styled(" 状态：", Style::new().fg(Color::DarkGray)),
            Span::styled(app.status.text.clone(), status_style(app.status.kind)),
        ]),
        Line::from(Span::styled(
            " ↑/↓ 选择菜单   Enter 打开   R 切换角色   Esc 退出登录   Q 退出程序 ",
            Style::new().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}
