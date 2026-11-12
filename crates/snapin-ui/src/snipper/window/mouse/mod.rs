//! 截图交互鼠标移动、按键点击、滚轮缩放与选区拖拽处理。

pub mod toolbar_action;

mod eraser;

mod annotation;
mod buttons;
mod long_capture;
mod movement;
mod ocr;
mod recording;
mod wheel;

pub use buttons::handle_mouse_button;
#[cfg(test)]
use buttons::{handle_left_press, handle_left_release};
pub(crate) use long_capture::toggle_long_capture_auto_scroll;
#[cfg(test)]
use movement::constrain_with_shift;
pub use movement::handle_cursor_move;
pub use wheel::handle_mouse_wheel;
pub(crate) use wheel::resolve_long_capture_wheel_delta;

use super::annotation_builder::commit_annotation;
use super::events::{commit_active_text_input, handle_escape, SnipperState};
use super::state::window_at;
use super::types::SnipMode;
use crate::editor::toolbar::{LineMode, Tool};
use crate::model::ShapeId;
use crate::snipper::selection::{hit_test_handle, DragMode, Rect, SelectionState};
use image::RgbaImage;
use snapin_annotation::AnnotationShape;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};

/// 鼠标按键交互事件对象，收敛散落的坐标、按键状态与图片引用。
#[derive(Clone, Copy)]
pub(super) struct PointerButtonEvent<'a> {
    pub button: MouseButton,
    pub button_state: ElementState,
    pub position: (f32, f32),
    pub original_rgba: &'a RgbaImage,
}

#[cfg(test)]
mod tests;
