//! 贴图窗口管理器与剪贴板便签化模块。

/// 贴图 ARGB8888 共享内存呈现通路的私有实现模块，仅经 `surface::ArgbShmSurface` 对外暴露
mod alpha_surface;
pub mod context_menu;
pub mod manager;
pub mod surface;
pub mod text_to_pin;
pub mod transform;
pub mod window;
pub mod workspace;

pub use manager::{PasterItem, PasterManager};
pub use surface::PasterSurface;
pub use text_to_pin::{render_color_swatch_card, render_simple_text_card, try_parse_hex_color};
pub use transform::PasterTransform;
pub use window::spawn_paster_window;
pub use workspace::{
    create_paster_workspace, run_paster_workspace_loop, spawn_paster_workspace_thread,
    PasterAppEvent, PasterWorkspaceProxy, WorkspaceStatus,
};
