//! 贴图浮动置顶窗口定义与单窗口兼容启动封装。

pub mod annotation;
pub mod events;
pub mod menu_actions;
pub mod render;
pub mod state;

pub use render::render_paster_to_surface;
pub use state::PasterWindowState;

use crate::error::UiError;
use crate::paster::manager::PasterItem;

/// 启动独立贴图浮动置顶窗口（兼容单次会话模式，自动由工作区托管并在全部关闭后退出）
pub fn spawn_paster_window(item: PasterItem) -> Result<(), UiError> {
    let (event_loop, proxy) = super::workspace::create_paster_workspace()?;
    proxy.open_paster(item)?;
    super::workspace::run_paster_workspace_loop(event_loop, true)
}
