//! 应用状态与事件循环。
//!
//! 界面路由很简单：未登录进入登录页，登录后按会话角色进入工作台。
//! 所有键盘输入在这里转成界面动作，业务调用统一走 [`crate::service`]。

use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::domain::user::{Role, Session, User};
use crate::error::Result;
use crate::repo::Store;
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
        Self::info("提示：Tab 切换输入框，Enter 登录，Esc 退出")
    }
}

/// 登录表单的输入焦点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginField {
    Account,
    Password,
}

/// 登录表单状态。
#[derive(Debug, Clone)]
pub struct LoginForm {
    pub account: String,
    pub password: String,
    pub field: LoginField,
}

impl LoginForm {
    pub fn new() -> Self {
        Self {
            account: String::new(),
            password: String::new(),
            field: LoginField::Account,
        }
    }

    /// 当前焦点对应的可变文本。
    fn active_buffer(&mut self) -> &mut String {
        match self.field {
            LoginField::Account => &mut self.account,
            LoginField::Password => &mut self.password,
        }
    }

    pub fn active_text(&self) -> &str {
        match self.field {
            LoginField::Account => &self.account,
            LoginField::Password => &self.password,
        }
    }

    fn insert(&mut self, ch: char) {
        self.active_buffer().push(ch);
    }

    fn backspace(&mut self) {
        self.active_buffer().pop();
    }

    fn next_field(&mut self) {
        self.field = match self.field {
            LoginField::Account => LoginField::Password,
            LoginField::Password => LoginField::Account,
        };
    }
}

impl Default for LoginForm {
    fn default() -> Self {
        Self::new()
    }
}

/// 应用全局状态。
pub struct App {
    /// 内存数据（后续由持久化层加载 / 回写）。
    pub store: Store,
    pub session: Option<Session>,
    pub view: View,
    pub running: bool,
    pub status: StatusMessage,
    /// 工作台左侧菜单的选中下标。
    pub menu_index: usize,
    pub login: LoginForm,
}

impl App {
    pub fn new() -> Self {
        Self {
            store: Store::new(),
            session: None,
            view: View::Login,
            running: true,
            status: StatusMessage::default(),
            menu_index: 0,
            login: LoginForm::new(),
        }
    }

    /// 事件循环：重绘 → 等待按键 → 处理。
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
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
            View::Login => self.on_login_key(key),
            View::Workspace => self.on_workspace_key(key),
        }
    }

    // ---------- 登录页 ----------

    fn on_login_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.running = false,
            KeyCode::Tab | KeyCode::BackTab => self.login.next_field(),
            KeyCode::Enter => self.submit_login(),
            KeyCode::Backspace => self.login.backspace(),
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.login.insert(ch)
            }
            // TODO(lab3): 骨架期临时入口，登录功能实现后删除。
            KeyCode::F(1) => self.enter_demo(Role::Buyer),
            KeyCode::F(2) => self.enter_demo(Role::Seller),
            KeyCode::F(3) => self.enter_demo(Role::Admin),
            _ => {}
        }
    }

    fn submit_login(&mut self) {
        match service::user::login(&mut self.store, &self.login.account, &self.login.password) {
            Ok(session) => {
                self.login.password.clear();
                self.enter_workspace(session);
            }
            Err(err) => self.set_status(StatusMessage::error(format!("登录失败：{err}"))),
        }
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
            KeyCode::F(1) => self.enter_demo(Role::Buyer),
            KeyCode::F(2) => self.enter_demo(Role::Seller),
            KeyCode::F(3) => self.enter_demo(Role::Admin),
            _ => {}
        }
    }

    fn logout(&mut self) {
        self.session = None;
        self.view = View::Login;
        self.menu_index = 0;
        self.set_status(StatusMessage::info("已退出登录"));
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
        self.set_status(StatusMessage::success(format!("已切换到{}视角", next.label())));
    }

    // ---------- 会话辅助 ----------

    /// 骨架期的演示会话：直接注入一个账号进入对应工作台。
    ///
    /// TODO(lab3): 登录 / 注册实现后移除该入口。
    fn enter_demo(&mut self, role: Role) {
        let mut user = User::new(format!("demo_{}", role.key()), None, None, "demo");
        user.buyer_role = true;
        user.seller_role = true;
        user.admin_role = matches!(role, Role::Admin);

        let session = Session::new(user.user_id.clone(), role);
        let username = user.username.clone();
        self.store.users.push(user);
        self.session = Some(session);
        self.view = View::Workspace;
        self.menu_index = 0;
        self.set_status(StatusMessage::info(format!(
            "演示会话已建立：{username}（{}），真实登录功能待实现",
            role.label()
        )));
    }

    fn enter_workspace(&mut self, session: Session) {
        self.session = Some(session);
        self.view = View::Workspace;
        self.menu_index = 0;
        self.set_status(StatusMessage::success("登录成功"));
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

    pub fn set_status(&mut self, status: StatusMessage) {
        self.status = status;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
