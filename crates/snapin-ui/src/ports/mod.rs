//! UI 与宿主环境之间的端口契约。
//!
//! # 设计原理
//!
//! 截图与贴图窗口是纯表现层，但用户的复制、保存、OCR 与翻译请求必须落到真正的
//! 平台适配与后台服务上。若 UI 直接依赖 `snapin-platform` 与 `snapin-service`，
//! 表现层就会被迫承担应用层编排职责：自己读配置、拼保存路径、维护 OCR 引擎缓存，
//! 最终导致同一套流程在截图与贴图两侧各写一遍。
//!
//! 本模块只声明“UI 需要宿主做什么”的最小契约，具体实现在 `snapin-app` 中：
//! - [`DesktopPort`]：选区缓存、系统剪贴板与文件管理器等桌面副作用。
//! - [`WorkspaceStateStore`]：持久化活动分组等运行时状态，不写入用户配置。
//! - [`MediaJobRunner`]：把裁剪图交给后台执行 OCR、翻译与保存，并通过回调把结果送回窗口事件循环。
//!
//! # 代价与权衡
//!
//! 端口层让 UI 可以脱离真实平台与网络独立测试，但引入了一次动态分发与一次回调装箱。
//! 这些开销只发生在用户动作触发的低频路径上，相对于 OCR 推理与图像编码可以忽略。

mod clipboard;
mod desktop;
mod error;
mod jobs;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use clipboard::noop_clipboard_event_sink;
pub use clipboard::{
    ClipboardEventSink, ClipboardJobId, ClipboardPayload, ClipboardResult, ClipboardSink,
    ClipboardTarget, ClipboardWrite,
};
pub use desktop::{DesktopPort, NoopDesktop, WorkspaceState, WorkspaceStateStore};
pub use error::{PortError, PortErrorKind};
pub use jobs::{JobCancel, JobId, MediaJob, MediaJobOutcome, MediaJobRunner, PasterJobKind};
pub use types::{OcrExportFormat, OcrTextItem, QrCodeItem};
