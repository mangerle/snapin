//! 截图交互与全屏选区覆盖子系统。

pub mod canvas;
pub mod magnifier;
pub mod selection;
pub mod toolbar;
pub mod window;

pub use canvas::SnipperCanvas;
pub use magnifier::{ColorFormat, Magnifier};
pub use selection::{Rect, SelectionHistory, SelectionState};
pub use toolbar::{StrokeWidthLevel, SubToolOptions, Toolbar, ToolbarItem};
pub use window::{run_snipper_session, SnipMode, SnipperOutcome};
