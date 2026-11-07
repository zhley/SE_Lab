//! 应用状态与事件循环。
//!
//! 界面路由很简单：未登录进入登录 / 注册页，登录后按会话角色进入工作台。
//! 所有键盘输入在这里转成界面动作，业务调用统一走 [`crate::service`]，
//! 数据变更后立即落盘（JSON 全量写入）。

use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::domain::user::{Role, Session, User};
use crate::error::{AppError, Result};
use crate::repo::persistence::{JsonFilePersistence, Persistence};
use crate::repo::{self, Store, seed};
use crate::service;
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
    pub auth: AuthForm,
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
                    Err(err) => {
                        StatusMessage::error(format!("初始化数据文件失败：{err}"))
                    }
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
            auth: AuthForm::new(),
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
        let result = service::user::login(
            &mut self.store,
            &self.auth.account,
            &self.auth.password,
        );

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
        let page_count = self.menu_len();
        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Esc => self.logout(),
            KeyCode::Up | KeyCode::Char('k') => {
                if page_count > 0 {
                    self.menu_index = (self.menu_index + page_count - 1) % page_count;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if page_count > 0 {
                    self.menu_index = (self.menu_index + 1) % page_count;
                }
            }
            KeyCode::Enter => {
                if let Some(page) = self.current_page() {
                    self.set_status(StatusMessage::info(format!(
                        "「{}」页面尚未实现（骨架阶段）",
                        page.title()
                    )));
                }
            }
            KeyCode::Char('r') => self.switch_role(),
            _ => {}
        }
    }

    fn logout(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = service::user::logout(&mut self.store, &session.token);
        }
        self.view = View::Login;
        self.menu_index = 0;
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
        self.set_status(StatusMessage::success(format!(
            "已切换到{}视角",
            next.label()
        )));
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
        self.set_status(StatusMessage::success(format!("登录成功，欢迎回来 {greeting}")));
    }

    // ---------- 会话辅助 ----------

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

    pub fn set_status(&mut self, status: StatusMessage) {
        self.status = status;
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
