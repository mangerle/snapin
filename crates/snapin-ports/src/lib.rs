//! Snapin 核心领域与宿主环境之间的端口契约（六边形架构 Ports 契约层）。
//!
//! # 设计原理
//!
//! 截图与贴图等表现层组件是纯图形视图，但用户的复制、保存、OCR 与翻译请求必须落到真正的
//! 平台适配与后台服务上。若 UI 直接依赖平台与服务实现，表现层就会被迫承担应用层编排职责：
//! 自己读配置、拼保存路径、维护 OCR 引擎缓存，最终导致同一套流程在不同表现层重复编写。
//!
//! 此外，宿主应用或后台守护进程如果直接依赖表现层，会导致庞大的 GUI 框架与图形驱动
//! 被非 GUI 服务无谓反向耦合。
//!
//! 本 crate 抽离纯粹的系统副作用与媒体异步任务契约接口：
//! - [`DesktopPort`]：选区缓存、系统剪贴板与文件管理器等桌面副作用。
//! - [`WorkspaceStateStore`]：持久化活动分组等运行时状态，不写入用户配置。
//! - [`MediaJobRunner`]：把裁剪图交给后台执行 OCR、翻译与保存，并通过回调把结果送回调用方。
//! - [`PasterId`] 与 [`GroupId`]：贴图窗口与贴图分组标识。
//!
//! # 代价与权衡
//!
//! 端口层让领域逻辑与表现层可以脱离真实平台与网络独立测试，但引入了一次动态分发与一次回调装箱。
//! 这些开销只发生在用户动作触发的低频路径上，相对于 OCR 推理与图像编码可以忽略。

mod clipboard;
mod desktop;
mod error;
mod id;
mod jobs;
mod types;

#[cfg(test)]
mod tests;

pub use clipboard::{
    noop_clipboard_event_sink, ClipboardEventSink, ClipboardJobId, ClipboardPayload,
    ClipboardResult, ClipboardSink, ClipboardTarget, ClipboardWrite,
};
pub use desktop::{DesktopPort, NoopDesktop, WorkspaceState, WorkspaceStateStore};
pub use error::{PortError, PortErrorKind};
pub use id::{GroupId, PasterId};
pub use jobs::{JobCancel, JobId, MediaJob, MediaJobOutcome, MediaJobRunner, PasterJobKind};
pub use types::{OcrExportFormat, OcrTextItem, QrCodeItem};
