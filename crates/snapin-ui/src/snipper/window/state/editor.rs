//! 截图会话的标注编辑状态。

use super::super::annotation_edit::AnnotationDrag;
use crate::editor::stroke_session::StrokeSession;
use crate::editor::text_input::TextInputSession;
use crate::editor::toolbar::Toolbar;
use crate::model::ShapeId;
use snapin_annotation::AnnotationHistory;

/// 工具栏、标注历史、笔迹与文字输入等编辑状态。
pub(crate) struct SnipperEditor {
    /// 当前工具栏状态。
    pub(crate) toolbar: Toolbar,
    /// 当前截图会话的标注历史。
    pub(crate) annotations: AnnotationHistory,
    /// 正在绘制的笔迹。
    pub(crate) stroke: StrokeSession,
    /// 当前文字输入会话。
    pub(crate) active_text_input: Option<TextInputSession>,
    /// 当前选中的已提交标注。
    pub(crate) selected_annotation: Option<ShapeId>,
    /// 标注拖动或手柄拉伸会话。
    pub(crate) annotation_drag: Option<AnnotationDrag>,
}

impl SnipperEditor {
    /// 使用指定工具栏构建编辑状态。
    pub(crate) fn new(toolbar: Toolbar) -> Self {
        Self {
            toolbar,
            annotations: AnnotationHistory::default(),
            stroke: StrokeSession::new(),
            active_text_input: None,
            selected_annotation: None,
            annotation_drag: None,
        }
    }
}
