//! Snapin 图形用户界面交互层（基于 Winit + Softbuffer + Tiny-Skia）。
//!
//! 提供基于纯 CPU 软件光栅化的全屏置顶截图交互（snipper）与常驻桌面贴图窗口（paster）。

mod drawing;
pub mod editor;
pub mod error;
pub mod geometry;
pub mod model;
pub mod paster;
pub mod ports;
pub mod snipper;
pub mod surface;

pub use error::UiError;
pub use ports::{
    ConfigStore, DesktopPort, JobCancel, JobId, MediaJob, MediaJobOutcome, MediaJobRunner,
    NoopDesktop, PortError,
};
pub use surface::blit_pixmap_to_surface;
pub use winit;
