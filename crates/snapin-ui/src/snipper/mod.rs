//! 截图交互与全屏选区覆盖子系统。

mod canvas;
mod magnifier;
mod selection;
pub(crate) mod window;
mod window_rect;

pub use crate::editor::toolbar::{
    StrokeWidthLevel, SubToolOptions, Tool, Toolbar, ToolbarAction, ToolbarItem,
};
pub use selection::{Rect, SelectionHistory, SelectionState};
pub use window::{SnipMode, SnipperLaunch, SnipperOutcome, SnipperRuntime, SnipperView};
pub use window_rect::WindowRect;
