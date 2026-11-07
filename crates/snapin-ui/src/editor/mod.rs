//! 可复用标注编辑组件（向下兼容重定向至 `snapin-editor`）。
//!
//! # 设计原理
//!
//! 通用标注工具栏、参数调控面板与文字输入会话已下沉至独立 crate [`snapin_editor`]。
//! 此模块全量重新导出 `snapin_editor`，保持 `snapin-ui` 内部模块与已有调用零破坏。

pub use snapin_editor::*;
