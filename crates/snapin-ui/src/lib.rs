//! Snapin 图形用户界面交互层（基于 Winit + Softbuffer + Tiny-Skia）。
//!
//! 提供基于纯 CPU 软件光栅化的全屏置顶截图交互（snipper）与常驻桌面贴图窗口（paster）。

mod drawing;
pub(crate) mod editor;
mod error;
pub(crate) mod geometry;
pub(crate) mod model;
pub mod paster;
pub(crate) mod ports;
pub mod snipper;
mod status_badge;
pub(crate) mod surface;
pub mod workspace;

pub use error::UiError;
pub use ports::{
    ClipboardJobId, ClipboardPayload, ClipboardResult, ClipboardSink, ClipboardWrite, DesktopPort,
    JobCancel, JobId, MediaJob, MediaJobOutcome, MediaJobRunner, OcrExportFormat, OcrTextItem,
    PortError, PortErrorKind, QrCodeItem, WorkspaceState, WorkspaceStateStore,
};
