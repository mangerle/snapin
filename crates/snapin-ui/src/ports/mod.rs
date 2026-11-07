//! UI 与宿主环境之间的端口契约（向下兼容重定向至 `snapin-ports`）。
//!
//! # 设计原理
//!
//! 契约已下沉至独立轻量 crate [`snapin_ports`]。为了保证 `snapin-ui` 内部与外部已有调用的无缝兼容，
//! 此模块全量重新导出 `snapin_ports` 的所有公有类型与特型。

pub use snapin_ports::*;
