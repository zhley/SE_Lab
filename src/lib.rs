//! 二手市集（软件工程课程实验三）。
//!
//! 校园二手交易平台，终端 TUI 交互，覆盖需求文档中的用户、商品审核、
//! 预约与交易、查询统计、通知与日志等业务。
//!
//! 分层结构：
//!
//! | 层次 | 模块 | 职责 |
//! | --- | --- | --- |
//! | 表现层 | [`tui`] / [`app`] | 终端界面、键盘事件、页面路由 |
//! | 应用服务层 | [`service`] | 业务规则、事务编排、权限校验 |
//! | 领域模型层 | [`domain`] | 实体、值对象、状态枚举 |
//! | 数据层 | [`repo`] | 内存数据与 JSON 文件持久化 |
//!
//! 通用能力：[`error`]（统一错误类型）、[`security`]（密码散列与输入校验）、
//! [`util`]（时间与编号生成）。

pub mod app;
pub mod domain;
pub mod error;
pub mod repo;
pub mod security;
pub mod service;
pub mod tui;
pub mod util;
