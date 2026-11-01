//! 截图会话标注草稿参数提取与提交适配。
//!
//! 可复用的图元工厂位于 [`crate::editor::annotation_builder`]；本模块只保留
//! 依赖截图会话状态的参数提取与提交逻辑，供画布即时预览与鼠标释放提交复用。

use super::events::SnipperState;
pub use crate::editor::annotation_builder::AnnotationDraftParams;

pub use crate::editor::annotation_builder::{build_shape, render_draft_preview};

impl<'a> AnnotationDraftParams<'a> {
    /// 从当前截图会话状态中提取草稿参数。
    pub fn from_state(state: &'a SnipperState) -> Option<Self> {
        let tool = state.toolbar.active_tool?;
        let points = &state.pending_stroke_points;
        Self::from_toolbar_and_points(
            &state.toolbar,
            tool,
            points,
            state.annotations.next_badge_number(),
        )
    }
}

/// 将当前暂存的笔触或图形提交为持久化标注图元。
pub fn commit_annotation(state: &mut SnipperState) {
    if let Some(params) = AnnotationDraftParams::from_state(state) {
        if let Some(shape) = build_shape(&params) {
            state.annotations.push(shape);
        }
    }
    state.pending_stroke_points.clear();
}
