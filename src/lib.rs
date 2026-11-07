//! 校园二手交易平台（软件工程课程实验三）。
//!
//! 分层结构：
//!
//! | 层次 | 模块 | 职责 |
//! | --- | --- | --- |
//! | 表现层 | [`tui`] / [`app`] | 终端界面、键盘事件、页面路由 |
//! | 应用服务层 | [`service`] | 业务规则、事务编排、权限校验 |
//! | 领域模型层 | [`domain`] | 实体、值对象、状态枚举 |
//! | 数据层 | [`repo`] | 内存数据存储与持久化接口 |
//!
//! 通用能力： [`error`]（统一错误类型）、[`util`]（时间与编号生成）。
//!
//! 当前阶段为**项目骨架**：数据结构、模块边界与接口已确定，
//! 具体业务逻辑以 `AppError::NotImplemented` 存根占位，按模块逐步实现。

pub mod app;
pub mod domain;
pub mod error;
pub mod repo;
pub mod service;
pub mod tui;
pub mod util;
