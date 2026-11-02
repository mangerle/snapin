//! 截图交互会话全屏窗口与双缓冲渲染事件循环。

pub mod actions;
pub mod annotation_builder;
pub mod annotation_edit;
pub(crate) mod clipboard;
pub mod cursor;
pub mod drag_calculator;
pub mod event_dispatcher;
pub mod events;
pub mod jobs;
pub mod mouse;
pub(crate) mod ocr_panel;
pub mod presenter;
mod presenter_support;
pub(crate) mod redaction;
pub mod state;
pub mod types;
pub mod view;

pub use event_dispatcher::SnipperEventDispatcher;
pub use presenter::{create_dimmed_pixmap_fast, SnipperPresenter};
pub use state::SnipperState;
pub use types::{SnipMode, SnipperLaunch, SnipperOutcome, SnipperRuntime};
pub use view::SnipperView;
