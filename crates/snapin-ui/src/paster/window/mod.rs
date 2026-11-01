//! 贴图浮动置顶窗口状态、渲染与交互定义。

pub mod annotation;
pub mod events;
mod input_region;
pub(crate) mod jobs;
pub mod menu_actions;
pub mod render;
pub mod state;
pub(crate) mod text_session;

pub use render::render_paster_to_surface;
pub use state::PasterWindowState;
