//! 截图交互会话全屏窗口与双缓冲渲染事件循环。

use self::events::SnipperState;
use crate::snipper::selection::SelectionState;
use winit::window::Window;

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
pub(crate) mod ocr_editor;
pub(crate) mod ocr_menu;
pub mod presenter;
mod presenter_support;
pub(crate) mod redaction;
pub mod state;
pub mod types;
pub mod view;

pub use types::{SnipMode, SnipperLaunch, SnipperOutcome, SnipperRuntime};
pub use view::SnipperView;

/// 按窗口实际缩放因子与屏幕尺寸初始化工具栏和放大镜缩放。
pub(super) fn apply_ui_scale(
    window: &Window,
    state: &mut SnipperState,
    screen_w: u32,
    screen_h: u32,
) {
    let raw_scale = window.scale_factor() as f32;
    let auto_scale = if raw_scale > 1.05 {
        raw_scale
    } else if screen_w >= 2560 {
        2.0
    } else {
        1.0
    } * state.session.config.interface.toolbar_scale;
    state.editor.annotation.toolbar.ui_scale = auto_scale;
    state.viewport.magnifier.ui_scale = auto_scale;

    if let SelectionState::Locked { rect, .. } = &state.viewport.selection {
        state
            .editor
            .annotation
            .toolbar
            .update_layout(rect, screen_w as f32, screen_h as f32);
    }
}
