//! Snapin 图形用户界面交互层（基于 Winit + Softbuffer + Tiny-Skia）。
//!
//! 提供基于纯 CPU 软件光栅化的全屏置顶截图交互（snipper）与常驻桌面贴图窗口（paster）。

pub mod paster;
pub mod snipper;
pub mod surface;

pub use surface::blit_pixmap_to_surface;
