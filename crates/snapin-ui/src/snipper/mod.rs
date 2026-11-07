//! 截图交互与全屏选区覆盖子系统。

mod canvas;
mod magnifier;
mod selection;
pub(crate) mod window;
mod window_rect;

pub use window::{SnipMode, SnipperLaunch, SnipperOutcome, SnipperPhase, SnipperRuntime};
pub use window_rect::WindowRect;
