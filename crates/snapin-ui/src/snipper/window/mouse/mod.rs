//! 截图交互鼠标移动、按键点击、滚轮缩放与选区拖拽处理。

pub mod toolbar_action;

mod eraser;

mod buttons;
mod movement;
mod wheel;

pub use buttons::handle_mouse_button;
#[cfg(test)]
use buttons::{handle_left_press, handle_left_release};
#[cfg(test)]
use movement::constrain_with_shift;
pub use movement::handle_cursor_move;
pub use wheel::handle_mouse_wheel;

use super::annotation_builder::commit_annotation;
use super::events::{commit_active_text_input, handle_escape, SnipperState};
use super::state::window_at;
use super::types::SnipMode;
use crate::editor::toolbar::{LineMode, Tool};
use crate::model::ShapeId;
use crate::snipper::selection::{hit_test_handle, DragMode, Rect, SelectionState};
use image::RgbaImage;
use snapin_annotation::AnnotationShape;
use toolbar_action::{dispatch_toolbar_action, handle_text_tool_click};
use winit::event::{ElementState, MouseButton, MouseScrollDelta};

#[cfg(test)]
mod tests;
