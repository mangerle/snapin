//! 截图交互会话全屏窗口与双缓冲渲染事件循环。

pub mod actions;
pub mod annotation_builder;
pub mod annotation_edit;
pub mod cursor;
pub mod drag_calculator;
pub mod event_dispatcher;
pub mod events;
pub mod jobs;
pub mod mouse;
pub mod presenter;
pub mod session;
pub mod state;
pub mod text_input;
pub mod types;

pub use event_dispatcher::SnipperEventDispatcher;
pub use presenter::{create_dimmed_pixmap_fast, SnipperPresenter};
pub use session::SnipperSession;
pub use state::SnipperState;
pub use types::{SnipMode, SnipperOutcome, SnipperRuntime};

use crate::error::UiError;
use crate::snipper::window_rect::WindowRect;
use image::RgbaImage;

/// 启动全屏截图交互会话，进入事件循环直至用户确认或取消
pub fn run_snipper_session(
    bg_image: RgbaImage,
    windows: Vec<WindowRect>,
    runtime: SnipperRuntime,
) -> Result<Option<SnipperOutcome>, UiError> {
    SnipperSession::new(bg_image, windows, runtime).run()
}
