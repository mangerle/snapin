//! 贴图窗口管理器与剪贴板便签化模块。

/// 贴图 ARGB8888 共享内存呈现通路的私有实现模块，仅经 `surface::ArgbShmSurface` 对外暴露
#[cfg(target_os = "linux")]
mod alpha_surface;

pub(crate) mod context_menu;
pub(crate) mod manager;
pub(crate) mod surface;
pub(crate) mod text_to_pin;
pub(crate) mod window;

/// 单帧呈现结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentOutcome {
    /// 本帧已提交给合成器。
    Presented,
    /// 合成器尚未归还可写缓冲区，本帧安全延期，稍后应重新请求重绘。
    Deferred,
}

pub use crate::model::{GroupId, PasterId, PasterItem, PasterTransform};
pub use text_to_pin::{render_color_swatch_card, render_simple_text_card, try_parse_hex_color};
