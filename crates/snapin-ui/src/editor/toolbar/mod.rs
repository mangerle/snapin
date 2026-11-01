//! 截图工具栏与二级属性选择器。
//!
//! 提供矩形、箭头、画笔、荧光笔、马赛克、模糊、聚光灯、文本等标注工具切换，
//! 以及撤销、重做、定在屏幕(贴图)、保存、复制、关闭等动作。
//! 支持自适应屏幕边缘检测，防止超出可视区域。

mod controller;
pub mod icons;
mod interaction;
pub mod items;
mod layout;
mod metrics;
pub mod render;

#[cfg(test)]
mod tests;

pub use items::{StrokeWidthLevel, SubToolOptions, Tool, ToolbarAction, ToolbarItem};
pub(crate) use layout::SubControl;
mod state;
pub use state::Toolbar;
