//! 贴图窗口管理器与剪贴板便签化模块。

/// 贴图 ARGB8888 共享内存呈现通路的私有实现模块，仅经 `surface::ArgbShmSurface` 对外暴露
#[cfg(target_os = "linux")]
mod alpha_surface;

pub mod context_menu;
pub mod manager;
pub mod surface;
pub mod text_to_pin;
pub mod window;
pub mod workspace;

/// 单帧呈现结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentOutcome {
    /// 本帧已提交给合成器。
    Presented,
    /// 合成器尚未归还可写缓冲区，本帧安全延期，稍后应重新请求重绘。
    Deferred,
}

pub use crate::model::{GroupId, PasterId, PasterItem, PasterTransform};
pub use manager::PasterManager;
pub use surface::PasterSurface;
pub use text_to_pin::{render_color_swatch_card, render_simple_text_card, try_parse_hex_color};
pub use workspace::{
    create_paster_workspace, is_valid_group_id, run_paster_workspace_loop, PasterAppEvent,
    PasterWorkspaceOptions, PasterWorkspaceProxy, SnipperFinishedCallback, WorkspaceStatus,
    TOTAL_PIN_GROUPS,
};
