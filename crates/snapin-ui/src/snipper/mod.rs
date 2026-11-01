//! 截图交互与全屏选区覆盖子系统。

pub mod canvas;
pub mod magnifier;
pub mod selection;
pub mod window;
pub mod window_rect;

pub use crate::editor::toolbar::{StrokeWidthLevel, SubToolOptions, Toolbar, ToolbarItem};
pub use canvas::SnipperCanvas;
pub use magnifier::{ColorFormat, Magnifier};
pub use selection::{Rect, SelectionHistory, SelectionState};
pub use window::{SnipMode, SnipperLaunch, SnipperOutcome, SnipperRuntime, SnipperView};
pub use window_rect::WindowRect;
