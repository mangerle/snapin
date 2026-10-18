//! 截图交互与全屏选区覆盖子系统。

pub mod canvas;
pub mod magnifier;
pub mod selection;

pub use canvas::SnipperCanvas;
pub use magnifier::{ColorFormat, Magnifier};
pub use selection::{Rect, SelectionHistory, SelectionState};
