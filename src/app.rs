//! 应用状态与事件循环。
//!
//! 界面路由：未登录进入登录 / 注册页，登录后按会话角色进入工作台。
//! 所有键盘输入在这里转成界面动作，业务调用统一走 [`crate::service`]，
//! 数据变更后立即落盘（JSON 全量写入）。

use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rust_decimal::Decimal;

use crate::domain::audit::Appeal;
use crate::domain::item::Item;
use crate::domain::log::OperationLog;
use crate::domain::notice::Notification;
use crate::domain::reservation::{Reservation, ReservationStatus};
use crate::domain::trade::Trade;
use crate::domain::user::{Role, Session, User};
use crate::error::{AppError, Result};
use crate::repo::persistence::{JsonFilePersistence, Persistence};
use crate::repo::{self, Store, seed};
use crate::service::{self, query};
use crate::tui;
use crate::tui::screens::workspace::Page;

/// 当前界面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// 登录 / 注册入口（访客）。
    Login,
    /// 登录后的角色工作台。
    Workspace,
}

/// 状态提示的语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Error,
}

/// 底部状态栏消息（对应 REQ-031：操作后立即给出明确提示）。
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub kind: StatusKind,
    pub text: String,
}

impl StatusMessage {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            kind: StatusKind::Info,
            text: text.into(),
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            kind: StatusKind::Success,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            kind: StatusKind::Error,
            text: text.into(),
        }
    }
}

impl Default for StatusMessage {
    fn default() -> Self {
        Self::info("提示：Tab 切换输入框，Enter 提交，F2 切换登录/注册")
    }
}

/// 认证页模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    Login,
    Register,
}

const LOGIN_LABELS: [&str; 2] = ["账号（用户名 / 手机号 / 邮箱）", "密码"];
const REGISTER_LABELS: [&str; 5] = [
    "用户名",
    "密码",
    "手机号（与邮箱至少填一项）",
    "邮箱",
    "验证码",
];

/// 登录 / 注册表单状态。
#[derive(Debug, Clone)]
pub struct AuthForm {
    pub mode: AuthMode,
    pub account: String,
    pub password: String,
    pub username: String,
    pub phone: String,
    pub email: String,
    pub captcha: String,
    /// 当前焦点字段下标。
    pub focus: usize,
}

impl AuthForm {
    pub fn new() -> Self {
        Self {
            mode: AuthMode::Login,
            account: String::new(),
            password: String::new(),
            username: String::new(),
            phone: String::new(),
            email: String::new(),
            captcha: String::new(),
            focus: 0,
        }
    }

    /// 当前模式下的字段标签。
    pub fn labels(&self) -> &'static [&'static str] {
        match self.mode {
            AuthMode::Login => &LOGIN_LABELS,
            AuthMode::Register => &REGISTER_LABELS,
        }
    }

    /// 按下标取字段值。
    pub fn value(&self, index: usize) -> &str {
        match (self.mode, index) {
            (AuthMode::Login, 0) => &self.account,
            (_, 1) => &self.password,
            (AuthMode::Register, 0) => &self.username,
            (AuthMode::Register, 2) => &self.phone,
            (AuthMode::Register, 3) => &self.email,
            (AuthMode::Register, 4) => &self.captcha,
            _ => "",
        }
    }

    /// 该字段是否需要遮蔽显示（密码）。
    pub fn masked(index: usize) -> bool {
        index == 1
    }

    fn buffer_mut(&mut self, index: usize) -> Option<&mut String> {
        match (self.mode, index) {
            (AuthMode::Login, 0) => Some(&mut self.account),
            (_, 1) => Some(&mut self.password),
            (AuthMode::Register, 0) => Some(&mut self.username),
            (AuthMode::Register, 2) => Some(&mut self.phone),
            (AuthMode::Register, 3) => Some(&mut self.email),
            (AuthMode::Register, 4) => Some(&mut self.captcha),
            _ => None,
        }
    }

    pub fn insert(&mut self, ch: char) {
        if let Some(buffer) = self.buffer_mut(self.focus) {
            buffer.push(ch);
        }
    }

    pub fn backspace(&mut self) {
        if let Some(buffer) = self.buffer_mut(self.focus) {
            buffer.pop();
        }
    }

    pub fn next_field(&mut self) {
        let count = self.labels().len();
        self.focus = (self.focus + 1) % count;
    }

    pub fn prev_field(&mut self) {
        let count = self.labels().len();
        self.focus = (self.focus + count - 1) % count;
    }

    /// 切换模式并重置焦点与临时输入。
    pub fn set_mode(&mut self, mode: AuthMode) {
        self.mode = mode;
        self.focus = 0;
    }

    /// 清空密码与验证码，避免残留在界面上。
    pub fn clear_sensitive(&mut self) {
        self.password.clear();
        self.captcha.clear();
    }
}

impl Default for AuthForm {
    fn default() -> Self {
        Self::new()
    }
}

/// 当前页面需要展示的数据快照。
#[derive(Debug, Default)]
pub enum PageData {
    #[default]
    Empty,
    Items(Vec<Item>),
    Reservations(Vec<Reservation>),
    Trades(Vec<Trade>),
    Notices(Vec<Notification>),
    Logs(Vec<OperationLog>),
    Appeals(Vec<Appeal>),
    Users(Vec<User>),
    /// 统计等纯文本内容。
    Lines(Vec<String>),
}

/// 商品编辑表单。
#[derive(Debug, Clone)]
pub struct ItemForm {
    /// `None` 表示新建。
    pub item_id: Option<String>,
    pub version: u64,
    pub title: String,
    pub price: String,
    pub description: String,
    pub category_index: usize,
    /// 0 标题 / 1 价格 / 2 描述 / 3 分类。
    pub focus: usize,
}

/// 需要输入文本的操作（原因、理由等）。
#[derive(Debug, Clone)]
pub enum InputAction {
    RejectItem(String),
    MarkViolation(String),
    BanUser(String),
    UnbanUser(String),
    Appeal(String),
}

/// 需要二次确认的高风险操作（REQ-030）。
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    ConfirmReservation {
        reservation_id: String,
        item_version: u64,
    },
    CancelReservation(String),
    CompleteTrade(String),
}

/// 弹窗。
#[derive(Debug, Clone)]
pub enum Dialog {
    Input {
        title: String,
        buffer: String,
        action: InputAction,
    },
    Confirm {
        title: String,
        action: ConfirmAction,
    },
}

impl Dialog {
    pub fn text(&self) -> &str {
        match self {
            Dialog::Input { title, .. } => title,
            Dialog::Confirm { title, .. } => title,
        }
    }
}

/// 应用全局状态。
pub struct App {
    /// 内存数据，变更后写回 JSON 文件。
    pub store: Store,
    pub session: Option<Session>,
    pub view: View,
    pub running: bool,
    pub status: StatusMessage,
    /// 工作台左侧菜单的选中下标。
    pub menu_index: usize,
    /// 列表选中下标。
    pub cursor: usize,
    pub data: PageData,
    pub auth: AuthForm,
    pub dialog: Option<Dialog>,
    pub item_form: Option<ItemForm>,
    persistence: JsonFilePersistence,
}

impl App {
    pub fn new() -> Self {
        let persistence = JsonFilePersistence::new(repo::default_data_path());

        let (store, status) = match persistence.load() {
            Ok(mut store) => {
                // 单用户桌面程序，启动时清理上次遗留的会话。
                store.sessions.clear();
                let user_count = store.users.len();
                (
                    store,
                    StatusMessage::info(format!(
                        "已加载数据：{}（账号 {user_count} 个）",
                        persistence.path.display()
                    )),
                )
            }
            Err(AppError::NotFound(_)) => {
                let store = seed::demo_store();
                let status = match persistence.save(&store) {
                    Ok(()) => StatusMessage::info(
                        "首次运行：已初始化演示数据，可用 admin / admin123 登录",
                    ),
                    Err(err) => StatusMessage::error(format!("初始化数据文件失败：{err}")),
                };
                (store, status)
            }
            Err(err) => (
                seed::demo_store(),
                StatusMessage::error(format!("读取数据失败：{err}；已使用临时演示数据")),
            ),
        };

        Self {
            store,
            session: None,
            view: View::Login,
            running: true,
            status,
            menu_index: 0,
            cursor: 0,
            data: PageData::Empty,
            auth: AuthForm::new(),
            dialog: None,
            item_form: None,
            persistence,
        }
    }

    /// 事件循环：重绘 → 等待按键 → 处理，退出前兜底落盘。
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let result = self.event_loop(terminal);
        if let Err(err) = self.persistence.save(&self.store) {
            eprintln!("退出时保存数据失败：{err}");
        }
        result
    }

    fn event_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while self.running {
            terminal.draw(|frame| tui::render(frame, self))?;
            self.tick()?;
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<()> {
        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            self.on_key(key);
        }
        Ok(())
    }

    /// 处理按键。
    pub fn on_key(&mut self, key: KeyEvent) {
        // Ctrl+C 任何时候都能退出。
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return;
        }

        match self.view {
            View::Login => self.on_auth_key(key),
            View::Workspace => self.on_workspace_key(key),
        }
    }

    // ---------- 登录 / 注册 ----------

    fn on_auth_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.on_auth_escape(),
            KeyCode::Tab | KeyCode::Down => self.auth.next_field(),
            KeyCode::BackTab | KeyCode::Up => self.auth.prev_field(),
            KeyCode::Enter => self.submit_auth(),
            KeyCode::Backspace => self.auth.backspace(),
            KeyCode::F(2) => self.toggle_auth_mode(),
            KeyCode::F(3) => self.request_captcha(),
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.auth.insert(ch)
            }
            _ => {}
        }
    }

    fn on_auth_escape(&mut self) {
        if self.auth.mode == AuthMode::Register {
            self.auth.set_mode(AuthMode::Login);
            self.set_status(StatusMessage::info("已返回登录"));
        } else {
            self.running = false;
        }
    }

    fn toggle_auth_mode(&mut self) {
        let mode = match self.auth.mode {
            AuthMode::Login => AuthMode::Register,
            AuthMode::Register => AuthMode::Login,
        };
        self.auth.set_mode(mode);
        self.auth.clear_sensitive();
        let hint = match mode {
            AuthMode::Login => "已切换到登录",
            AuthMode::Register => "已切换到注册：填写手机号或邮箱后按 F3 获取验证码",
        };
        self.set_status(StatusMessage::info(hint));
    }

    fn submit_auth(&mut self) {
        match self.auth.mode {
            AuthMode::Login => self.submit_login(),
            AuthMode::Register => self.submit_register(),
        }
    }

    fn submit_login(&mut self) {
        let result = service::user::login(&mut self.store, &self.auth.account, &self.auth.password);

        match result {
            Ok(session) => {
                self.auth.clear_sensitive();
                self.enter_workspace(session);
            }
            Err(err) => self.set_status(StatusMessage::error(format!("登录失败：{err}"))),
        }
        self.persist();
    }

    fn submit_register(&mut self) {
        let request = service::user::RegisterRequest {
            username: self.auth.username.trim().to_string(),
            password: self.auth.password.clone(),
            phone: Some(self.auth.phone.trim().to_string()).filter(|text| !text.is_empty()),
            email: Some(self.auth.email.trim().to_string()).filter(|text| !text.is_empty()),
            captcha: self.auth.captcha.trim().to_string(),
        };

        let result = service::user::register(&mut self.store, &request);
        match result {
            Ok(user) => {
                let username = user.username.clone();
                self.auth.set_mode(AuthMode::Login);
                self.auth.account = username.clone();
                self.auth.clear_sensitive();
                self.set_status(StatusMessage::success(format!(
                    "注册成功，请使用「{username}」登录"
                )));
            }
            Err(err) => self.set_status(StatusMessage::error(format!("注册失败：{err}"))),
        }
        self.persist();
    }

    fn request_captcha(&mut self) {
        if self.auth.mode != AuthMode::Register {
            self.set_status(StatusMessage::info("请先按 F2 切换到注册"));
            return;
        }

        let target = if !self.auth.phone.trim().is_empty() {
            self.auth.phone.trim().to_string()
        } else {
            self.auth.email.trim().to_string()
        };

        match service::user::issue_captcha(&mut self.store, &target) {
            Ok(code) => self.set_status(StatusMessage::success(format!(
                "验证码：{code}（5 分钟内有效；演示环境不发送短信 / 邮件）"
            ))),
            Err(err) => self.set_status(StatusMessage::error(format!("获取验证码失败：{err}"))),
        }
        self.persist();
    }

    // ---------- 工作台 ----------

    fn on_workspace_key(&mut self, key: KeyEvent) {
        if self.dialog.is_some() {
            self.on_dialog_key(key);
            return;
        }
        if self.item_form.is_some() {
            self.on_form_key(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Esc => self.logout(),
            KeyCode::Tab | KeyCode::Right => self.switch_page(1),
            KeyCode::BackTab | KeyCode::Left => self.switch_page(-1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::Enter => self.primary_action(),
            KeyCode::F(4) => self.switch_role(),
            KeyCode::Char(ch) => self.page_action(ch),
            _ => {}
        }
    }

    fn switch_page(&mut self, step: isize) {
        let count = self.menu_len() as isize;
        if count == 0 {
            return;
        }
        self.menu_index = ((self.menu_index as isize + step).rem_euclid(count)) as usize;
        self.cursor = 0;
        self.dialog = None;
        self.item_form = None;
        self.refresh();
    }

    fn move_cursor(&mut self, step: isize) {
        let len = self.data_len();
        if len == 0 {
            return;
        }
        self.cursor = ((self.cursor as isize + step).rem_euclid(len as isize)) as usize;
    }

    fn data_len(&self) -> usize {
        match &self.data {
            PageData::Items(list) => list.len(),
            PageData::Reservations(list) => list.len(),
            PageData::Trades(list) => list.len(),
            PageData::Notices(list) => list.len(),
            PageData::Logs(list) => list.len(),
            PageData::Appeals(list) => list.len(),
            PageData::Users(list) => list.len(),
            PageData::Lines(_) | PageData::Empty => 0,
        }
    }

    /// Enter 键的主操作，按页面语义决定。
    fn primary_action(&mut self) {
        match self.current_page() {
            Some(Page::BrowseItems) => self.reserve_selected(),
            Some(Page::MyItems) => self.edit_selected(),
            Some(Page::NewItem) => self.open_item_form(None),
            Some(Page::SellerReservations) => self.request_confirm_reservation(),
            Some(Page::Notifications) => self.mark_notice_read(),
            Some(Page::AdminAudit) => self.approve_selected(),
            Some(Page::AdminAppeals) => self.handle_appeal_selected(true),
            _ => {}
        }
    }

    /// 字母键的页面操作。
    fn page_action(&mut self, ch: char) {
        match (self.current_page(), ch) {
            (Some(Page::MyReservations), 'c') => self.request_cancel_reservation(),
            (Some(Page::MyTrades), 'c') => self.confirm_trade_selected(),
            (Some(Page::MyItems), 'n') => self.open_item_form(None),
            (Some(Page::MyItems), 'e') => self.edit_selected(),
            (Some(Page::MyItems), 's') => self.submit_audit_selected(),
            (Some(Page::MyItems), 'o') => self.take_offline_selected(),
            (Some(Page::MyItems), 'p') => self.appeal_selected(),
            (Some(Page::SellerReservations), 'c') => self.request_cancel_reservation(),
            (Some(Page::SellerTrades), 'c') => self.request_complete_trade(),
            (Some(Page::Notifications), 'r') => self.mark_notice_read(),
            (Some(Page::Notifications), 'a') => self.mark_all_notices(),
            (Some(Page::AdminAudit), 'y') => self.approve_selected(),
            (Some(Page::AdminAudit), 'n') => self.reject_selected(),
            (Some(Page::AdminViolations), 'v') => self.violate_selected(),
            (Some(Page::AdminAppeals), 'y') => self.handle_appeal_selected(true),
            (Some(Page::AdminAppeals), 'n') => self.handle_appeal_selected(false),
            (Some(Page::AdminUsers), 'b') => self.ban_selected(),
            (Some(Page::AdminUsers), 'u') => self.unban_selected(),
            (Some(Page::AdminUsers), 'l') => self.unlock_selected(),
            _ => {}
        }
    }

    // ---------- 业务动作 ----------

    fn reserve_selected(&mut self) {
        let Some(item) = self.selected_item() else {
            self.set_status(StatusMessage::info("当前列表没有商品"));
            return;
        };
        let user_id = self.user_id();
        let result = service::trade::create_reservation(
            &mut self.store,
            &user_id,
            &item.item_id,
            item.version,
        );
        self.report(result.map(|_| "预约成功，等待卖家确认".to_string()));
    }

    fn request_confirm_reservation(&mut self) {
        let Some(reservation) = self.selected_reservation() else {
            self.set_status(StatusMessage::info("没有可确认的预约"));
            return;
        };
        let Some(item) = self.store.item(&reservation.item_id) else {
            self.set_status(StatusMessage::error("商品不存在"));
            return;
        };
        let title = format!(
            "确认预约？商品「{}」将进入交易中，其余待确认预约自动失效 (y/n)",
            item.title
        );
        self.dialog = Some(Dialog::Confirm {
            title,
            action: ConfirmAction::ConfirmReservation {
                reservation_id: reservation.reservation_id.clone(),
                item_version: item.version,
            },
        });
    }

    fn request_cancel_reservation(&mut self) {
        let Some(reservation) = self.selected_reservation() else {
            self.set_status(StatusMessage::info("没有选中的预约"));
            return;
        };
        if reservation.status == ReservationStatus::Confirmed {
            self.dialog = Some(Dialog::Confirm {
                title: "取消已确认的预约？商品将恢复为已发布 (y/n)".into(),
                action: ConfirmAction::CancelReservation(reservation.reservation_id.clone()),
            });
        } else {
            let user_id = self.user_id();
            let result = service::trade::cancel_reservation(
                &mut self.store,
                &user_id,
                &reservation.reservation_id,
            );
            self.report(result.map(|_| "预约已取消".to_string()));
        }
    }

    fn confirm_trade_selected(&mut self) {
        let Some(trade) = self.selected_trade() else {
            self.set_status(StatusMessage::info("没有选中的交易"));
            return;
        };
        let user_id = self.user_id();
        let result =
            service::trade::confirm_trade_by_buyer(&mut self.store, &user_id, &trade.trade_id);
        self.report(result.map(|_| "已确认交易".to_string()));
    }

    fn request_complete_trade(&mut self) {
        let Some(trade) = self.selected_trade() else {
            self.set_status(StatusMessage::info("没有选中的交易"));
            return;
        };
        self.dialog = Some(Dialog::Confirm {
            title: "标记交易完成？商品将变为已售出，不可撤销 (y/n)".into(),
            action: ConfirmAction::CompleteTrade(trade.trade_id.clone()),
        });
    }

    fn submit_audit_selected(&mut self) {
        let Some(item) = self.selected_item() else {
            self.set_status(StatusMessage::info("没有选中的商品"));
            return;
        };
        let user_id = self.user_id();
        let result = service::item::submit_audit(&mut self.store, &user_id, &item.item_id);
        self.report(result.map(|_| "已提交审核".to_string()));
    }

    fn take_offline_selected(&mut self) {
        let Some(item) = self.selected_item() else {
            self.set_status(StatusMessage::info("没有选中的商品"));
            return;
        };
        let user_id = self.user_id();
        let result = service::item::take_offline(&mut self.store, &user_id, &item.item_id);
        self.report(result.map(|_| "商品已下架，待确认预约自动失效".to_string()));
    }

    fn edit_selected(&mut self) {
        let Some(item) = self.selected_item() else {
            self.set_status(StatusMessage::info("当前列表没有商品"));
            return;
        };
        if !item.status.editable() {
            self.set_status(StatusMessage::info(format!(
                "「{}」状态的商品不可编辑",
                item.status.label()
            )));
            return;
        }
        self.open_item_form(Some(item));
    }

    fn appeal_selected(&mut self) {
        let Some(item) = self.selected_item() else {
            self.set_status(StatusMessage::info("没有选中的商品"));
            return;
        };
        let violation = self
            .store
            .violations
            .iter()
            .rev()
            .find(|record| record.item_id == item.item_id)
            .map(|record| record.violation_id.clone());
        let Some(violation_id) = violation else {
            self.set_status(StatusMessage::info("该商品没有违规记录"));
            return;
        };
        self.dialog = Some(Dialog::Input {
            title: "申诉理由（Enter 提交，Esc 取消）".into(),
            buffer: String::new(),
            action: InputAction::Appeal(violation_id),
        });
    }

    fn approve_selected(&mut self) {
        let Some(item) = self.selected_item() else {
            self.set_status(StatusMessage::info("没有待审核商品"));
            return;
        };
        let user_id = self.user_id();
        let result = service::item::approve(&mut self.store, &user_id, &item.item_id);
        self.report(result.map(|_| "已审核通过，商品发布".to_string()));
    }

    fn reject_selected(&mut self) {
        let Some(item) = self.selected_item() else {
            self.set_status(StatusMessage::info("没有待审核商品"));
            return;
        };
        self.dialog = Some(Dialog::Input {
            title: format!("驳回理由（商品：{}）", item.title),
            buffer: String::new(),
            action: InputAction::RejectItem(item.item_id),
        });
    }

    fn violate_selected(&mut self) {
        let Some(item) = self.selected_item() else {
            self.set_status(StatusMessage::info("没有可选商品"));
            return;
        };
        self.dialog = Some(Dialog::Input {
            title: format!("违规原因（商品：{}）", item.title),
            buffer: String::new(),
            action: InputAction::MarkViolation(item.item_id),
        });
    }

    fn ban_selected(&mut self) {
        let Some(user) = self.selected_user() else {
            self.set_status(StatusMessage::info("没有选中的用户"));
            return;
        };
        self.dialog = Some(Dialog::Input {
            title: format!("封禁理由（用户：{}）", user.username),
            buffer: String::new(),
            action: InputAction::BanUser(user.user_id),
        });
    }

    fn unban_selected(&mut self) {
        let Some(user) = self.selected_user() else {
            self.set_status(StatusMessage::info("没有选中的用户"));
            return;
        };
        self.dialog = Some(Dialog::Input {
            title: format!("解封理由（用户：{}）", user.username),
            buffer: String::new(),
            action: InputAction::UnbanUser(user.user_id),
        });
    }

    fn unlock_selected(&mut self) {
        let Some(user) = self.selected_user() else {
            self.set_status(StatusMessage::info("没有选中的用户"));
            return;
        };
        let admin_id = self.user_id();
        let user_id = user.user_id.clone();
        let result = service::user::unlock_login(&mut self.store, &admin_id, &user_id);
        self.report(result.map(|_| "已解除登录锁定".to_string()));
    }

    fn handle_appeal_selected(&mut self, accept: bool) {
        let Some(appeal) = self.selected_appeal() else {
            self.set_status(StatusMessage::info("没有待处理申诉"));
            return;
        };
        let admin_id = self.user_id();
        let result =
            service::item::handle_appeal(&mut self.store, &admin_id, &appeal.appeal_id, accept);
        self.report(result.map(|_| {
            if accept {
                "申诉已受理，商品回到待审核".to_string()
            } else {
                "申诉已驳回".to_string()
            }
        }));
    }

    fn mark_notice_read(&mut self) {
        let Some(notice) = self.selected_notice() else {
            self.set_status(StatusMessage::info("没有通知"));
            return;
        };
        let user_id = self.user_id();
        let result = service::notice::mark_read(&mut self.store, &user_id, &notice.notice_id);
        self.report(result.map(|_| "已标记为已读".to_string()));
    }

    fn mark_all_notices(&mut self) {
        let user_id = self.user_id();
        let result = service::notice::mark_all_read(&mut self.store, &user_id);
        self.report(result.map(|count| format!("已标记 {count} 条通知为已读")));
    }

    // ---------- 弹窗 ----------

    fn on_dialog_key(&mut self, key: KeyEvent) {
        let is_input = matches!(self.dialog, Some(Dialog::Input { .. }));

        if is_input {
            match key.code {
                KeyCode::Esc => {
                    self.dialog = None;
                    self.set_status(StatusMessage::info("已取消"));
                    return;
                }
                KeyCode::Backspace => {
                    if let Some(Dialog::Input { buffer, .. }) = &mut self.dialog {
                        buffer.pop();
                    }
                    return;
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(Dialog::Input { buffer, .. }) = &mut self.dialog {
                        buffer.push(ch);
                    }
                    return;
                }
                KeyCode::Enter => {}
                _ => return,
            }
        }

        let Some(dialog) = self.dialog.take() else {
            return;
        };
        match dialog {
            Dialog::Input { action, buffer, .. } => {
                self.run_input(action, buffer.trim().to_string())
            }
            Dialog::Confirm { action, .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.run_confirm(action)
                }
                _ => self.set_status(StatusMessage::info("已取消操作")),
            },
        }
    }

    fn run_input(&mut self, action: InputAction, text: String) {
        let user_id = self.user_id();
        let result = match action {
            InputAction::RejectItem(item_id) => {
                service::item::reject(&mut self.store, &user_id, &item_id, &text)
                    .map(|_| "已驳回该商品".to_string())
            }
            InputAction::MarkViolation(item_id) => {
                service::item::mark_violation(&mut self.store, &user_id, &item_id, &text)
                    .map(|_| "已违规下架，卖家可在 7 天内申诉".to_string())
            }
            InputAction::BanUser(target_id) => {
                service::user::ban_user(&mut self.store, &user_id, &target_id, &text)
                    .map(|_| "已封禁该用户".to_string())
            }
            InputAction::UnbanUser(target_id) => {
                service::user::unban_user(&mut self.store, &user_id, &target_id, &text)
                    .map(|_| "已解封该用户".to_string())
            }
            InputAction::Appeal(violation_id) => {
                service::item::submit_appeal(&mut self.store, &user_id, &violation_id, &text)
                    .map(|_| "申诉已提交，等待管理员处理".to_string())
            }
        };
        self.report(result);
    }

    fn run_confirm(&mut self, action: ConfirmAction) {
        let user_id = self.user_id();
        let result = match action {
            ConfirmAction::ConfirmReservation {
                reservation_id,
                item_version,
            } => service::trade::confirm_reservation(
                &mut self.store,
                &user_id,
                &reservation_id,
                item_version,
            )
            .map(|_| "已确认预约，商品进入交易中".to_string()),
            ConfirmAction::CancelReservation(reservation_id) => {
                service::trade::cancel_reservation(&mut self.store, &user_id, &reservation_id)
                    .map(|_| "预约已取消".to_string())
            }
            ConfirmAction::CompleteTrade(trade_id) => {
                service::trade::complete_trade(&mut self.store, &user_id, &trade_id)
                    .map(|_| "交易已完成，商品已售出".to_string())
            }
        };
        self.report(result);
    }

    // ---------- 商品表单 ----------

    fn open_item_form(&mut self, existing: Option<Item>) {
        let categories = query::categories(&self.store);
        if categories.is_empty() {
            self.set_status(StatusMessage::error("没有可用分类"));
            return;
        }
        let category_index = existing
            .as_ref()
            .and_then(|item| {
                categories
                    .iter()
                    .position(|category| category.category_id == item.category_id)
            })
            .unwrap_or(0);

        self.item_form = Some(ItemForm {
            item_id: existing.as_ref().map(|item| item.item_id.clone()),
            version: existing.as_ref().map(|item| item.version).unwrap_or(1),
            title: existing
                .as_ref()
                .map(|item| item.title.clone())
                .unwrap_or_default(),
            price: existing
                .as_ref()
                .map(|item| item.price.to_string())
                .unwrap_or_default(),
            description: existing
                .as_ref()
                .map(|item| item.description.clone())
                .unwrap_or_default(),
            category_index,
            focus: 0,
        });
    }

    fn on_form_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.item_form = None;
                self.set_status(StatusMessage::info("已取消编辑"));
            }
            KeyCode::Tab | KeyCode::Down => {
                if let Some(form) = &mut self.item_form {
                    form.focus = (form.focus + 1) % 4;
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(form) = &mut self.item_form {
                    form.focus = (form.focus + 3) % 4;
                }
            }
            KeyCode::Left => {
                if let Some(form) = &mut self.item_form
                    && form.focus == 3
                {
                    form.category_index = form.category_index.saturating_sub(1);
                }
            }
            KeyCode::Right => {
                let count = query::categories(&self.store).len();
                if let Some(form) = &mut self.item_form
                    && form.focus == 3
                    && count > 0
                {
                    form.category_index = (form.category_index + 1).min(count - 1);
                }
            }
            KeyCode::Enter => self.submit_item_form(false),
            KeyCode::F(2) => self.submit_item_form(true),
            KeyCode::Backspace => {
                if let Some(form) = &mut self.item_form {
                    match form.focus {
                        0 => {
                            form.title.pop();
                        }
                        1 => {
                            form.price.pop();
                        }
                        2 => {
                            form.description.pop();
                        }
                        _ => {}
                    }
                }
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(form) = &mut self.item_form {
                    match form.focus {
                        0 => form.title.push(ch),
                        1 => form.price.push(ch),
                        2 => form.description.push(ch),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn submit_item_form(&mut self, submit: bool) {
        let Some(form) = self.item_form.clone() else {
            return;
        };
        let categories = query::categories(&self.store);
        let Some(category) = categories.get(form.category_index) else {
            self.set_status(StatusMessage::error("请选择商品分类"));
            return;
        };
        let price = match form.price.trim().parse::<Decimal>() {
            Ok(price) => price,
            Err(_) => {
                self.set_status(StatusMessage::error("价格必须是数字"));
                return;
            }
        };

        let draft = service::item::ItemDraft {
            title: form.title.clone(),
            description: form.description.clone(),
            category_id: category.category_id.clone(),
            price: Some(price),
        };
        let user_id = self.user_id();

        let saved = match &form.item_id {
            Some(item_id) => {
                service::item::update_item(&mut self.store, &user_id, item_id, &draft, form.version)
                    .map(|_| item_id.clone())
            }
            None => service::item::save_draft(&mut self.store, &user_id, &draft)
                .map(|item| item.item_id),
        };

        match saved {
            Ok(item_id) => {
                self.item_form = None;
                if submit {
                    let result = service::item::submit_audit(&mut self.store, &user_id, &item_id);
                    self.report(result.map(|_| "已提交审核，等待管理员处理".to_string()));
                } else {
                    self.set_status(StatusMessage::success("已保存草稿"));
                    self.after_change();
                }
            }
            Err(err) => {
                self.set_status(StatusMessage::error(format!("保存失败：{err}")));
            }
        }
    }

    // ---------- 数据加载 ----------

    /// 重新加载当前页面的数据。
    pub fn refresh(&mut self) {
        let Some(session) = self.session.clone() else {
            self.data = PageData::Empty;
            return;
        };
        let user_id = session.user_id.clone();

        self.data = match self.current_page() {
            Some(Page::BrowseItems) => {
                let filter = query::ItemFilter::default();
                match query::search_items(
                    &self.store,
                    &session,
                    &filter,
                    query::SortBy::CreatedAt,
                    query::SortOrder::Desc,
                    1,
                ) {
                    Ok(page) => PageData::Items(page.items),
                    Err(err) => {
                        self.status = StatusMessage::error(format!("查询失败：{err}"));
                        PageData::Empty
                    }
                }
            }
            Some(Page::MyReservations) => {
                PageData::Reservations(query::reservations_of(&self.store, &user_id, false))
            }
            Some(Page::MyTrades) => {
                PageData::Trades(query::trades_of(&self.store, &user_id, false))
            }
            Some(Page::MyItems) => {
                let mut items: Vec<Item> = self.store.items_of_seller(&user_id).cloned().collect();
                items.sort_by_key(|item| std::cmp::Reverse(item.created_at));
                PageData::Items(items)
            }
            Some(Page::NewItem) => PageData::Empty,
            Some(Page::SellerReservations) => {
                PageData::Reservations(query::reservations_of(&self.store, &user_id, true))
            }
            Some(Page::SellerTrades) => {
                PageData::Trades(query::trades_of(&self.store, &user_id, true))
            }
            Some(Page::SellerStats) => match query::seller_stats(&self.store, &user_id) {
                Ok(stats) => PageData::Lines(vec![
                    format!("在售商品：{}", stats.published),
                    format!("已售商品：{}", stats.sold),
                    format!("下架 / 违规：{}", stats.offline),
                    format!("有效预约：{}", stats.reservations),
                ]),
                Err(err) => PageData::Lines(vec![format!("统计失败：{err}")]),
            },
            Some(Page::Notifications) => match service::notice::list(&self.store, &user_id) {
                Ok(list) => PageData::Notices(list),
                Err(err) => {
                    self.status = StatusMessage::error(format!("读取通知失败：{err}"));
                    PageData::Empty
                }
            },
            Some(Page::AdminAudit) => PageData::Items(query::pending_items(&self.store)),
            Some(Page::AdminViolations) => PageData::Items(query::published_items(&self.store)),
            Some(Page::AdminAppeals) => PageData::Appeals(query::pending_appeals(&self.store)),
            Some(Page::AdminUsers) => PageData::Users(query::users(&self.store)),
            Some(Page::AdminLogs) => {
                let log_query = service::log::LogQuery {
                    page: 1,
                    page_size: 50,
                    ..Default::default()
                };
                match service::log::query(&self.store, &user_id, &log_query) {
                    Ok(logs) => PageData::Logs(logs),
                    Err(err) => {
                        self.status = StatusMessage::error(format!("读取日志失败：{err}"));
                        PageData::Empty
                    }
                }
            }
            Some(Page::AdminStats) => match query::admin_stats(&self.store) {
                Ok(stats) => {
                    let mut lines = vec![
                        format!("待审核商品：{}", stats.pending_items),
                        format!("违规下架商品：{}", stats.violation_items),
                        "按分类统计已售商品：".to_string(),
                    ];
                    if stats.sold_by_category.is_empty() {
                        lines.push("  （暂无数据）".to_string());
                    } else {
                        for (name, count) in stats.sold_by_category {
                            lines.push(format!("  {name}：{count}"));
                        }
                    }
                    PageData::Lines(lines)
                }
                Err(err) => PageData::Lines(vec![format!("统计失败：{err}")]),
            },
            None => PageData::Empty,
        };

        let len = self.data_len();
        if len == 0 {
            self.cursor = 0;
        } else {
            self.cursor = self.cursor.min(len - 1);
        }
    }

    /// 业务动作完成后统一收尾：落盘并刷新列表。
    fn after_change(&mut self) {
        self.persist();
        self.refresh();
    }

    fn report(&mut self, result: Result<String>) {
        match result {
            Ok(message) => self.set_status(StatusMessage::success(message)),
            Err(err) => self.set_status(StatusMessage::error(err.to_string())),
        }
        self.after_change();
    }

    // ---------- 选中项 ----------

    pub fn selected_item(&self) -> Option<Item> {
        match &self.data {
            PageData::Items(items) => items.get(self.cursor).cloned(),
            _ => None,
        }
    }

    pub fn selected_reservation(&self) -> Option<Reservation> {
        match &self.data {
            PageData::Reservations(list) => list.get(self.cursor).cloned(),
            _ => None,
        }
    }

    pub fn selected_trade(&self) -> Option<Trade> {
        match &self.data {
            PageData::Trades(list) => list.get(self.cursor).cloned(),
            _ => None,
        }
    }

    pub fn selected_notice(&self) -> Option<Notification> {
        match &self.data {
            PageData::Notices(list) => list.get(self.cursor).cloned(),
            _ => None,
        }
    }

    pub fn selected_user(&self) -> Option<User> {
        match &self.data {
            PageData::Users(list) => list.get(self.cursor).cloned(),
            _ => None,
        }
    }

    pub fn selected_appeal(&self) -> Option<Appeal> {
        match &self.data {
            PageData::Appeals(list) => list.get(self.cursor).cloned(),
            _ => None,
        }
    }

    // ---------- 辅助 ----------

    fn user_id(&self) -> String {
        self.session
            .as_ref()
            .map(|session| session.user_id.clone())
            .unwrap_or_default()
    }

    /// 当前登录用户。
    pub fn current_user(&self) -> Option<&User> {
        let session = self.session.as_ref()?;
        self.store.user(&session.user_id)
    }

    /// 当前会话角色。
    pub fn current_role(&self) -> Option<Role> {
        self.session.as_ref().map(|session| session.role)
    }

    /// 当前角色的菜单项。
    pub fn menu(&self) -> &'static [Page] {
        self.current_role().map(Page::menu).unwrap_or(&[])
    }

    fn menu_len(&self) -> usize {
        self.menu().len()
    }

    /// 当前选中的页面。
    pub fn current_page(&self) -> Option<Page> {
        self.menu().get(self.menu_index).copied()
    }

    /// 未读通知数（顶栏展示）。
    pub fn unread_notices(&self) -> usize {
        self.current_user()
            .map(|user| self.store.unread_count(&user.user_id))
            .unwrap_or(0)
    }

    pub fn username_of(&self, user_id: &str) -> String {
        self.store
            .user(user_id)
            .map(|user| user.username.clone())
            .unwrap_or_else(|| "-".to_string())
    }

    pub fn item_title_of(&self, item_id: &str) -> String {
        self.store
            .item(item_id)
            .map(|item| item.title.clone())
            .unwrap_or_else(|| "（商品已删除）".to_string())
    }

    pub fn set_status(&mut self, status: StatusMessage) {
        self.status = status;
    }

    fn logout(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = service::user::logout(&mut self.store, &session.token);
        }
        self.view = View::Login;
        self.menu_index = 0;
        self.cursor = 0;
        self.data = PageData::Empty;
        self.dialog = None;
        self.item_form = None;
        self.auth.set_mode(AuthMode::Login);
        self.auth.clear_sensitive();
        self.set_status(StatusMessage::info("已退出登录"));
        self.persist();
    }

    /// 在账号具备的多个角色之间切换（同一账号可同时是买家和卖家）。
    fn switch_role(&mut self) {
        let Some((roles, current)) = self.session.as_ref().and_then(|session| {
            let user = self.store.user(&session.user_id)?;
            Some((user.roles(), session.role))
        }) else {
            return;
        };

        if roles.len() <= 1 {
            self.set_status(StatusMessage::info("当前账号只有一个角色"));
            return;
        }

        let index = roles.iter().position(|role| *role == current).unwrap_or(0);
        let next = roles[(index + 1) % roles.len()];
        if let Some(session) = self.session.as_mut() {
            session.role = next;
        }
        self.menu_index = 0;
        self.cursor = 0;
        self.set_status(StatusMessage::success(format!(
            "已切换到{}视角",
            next.label()
        )));
        self.refresh();
    }

    fn enter_workspace(&mut self, session: Session) {
        let greeting = self
            .store
            .user(&session.user_id)
            .map(|user| user.username.clone())
            .unwrap_or_default();
        self.session = Some(session);
        self.view = View::Workspace;
        self.menu_index = 0;
        self.cursor = 0;
        self.set_status(StatusMessage::success(format!(
            "登录成功，欢迎回来 {greeting}"
        )));
        self.refresh();
    }

    /// 立即写回磁盘；失败时在状态栏提示（不中断当前操作）。
    fn persist(&mut self) {
        if let Err(err) = self.persistence.save(&self.store) {
            self.status = StatusMessage::error(format!("数据保存失败：{err}"));
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
