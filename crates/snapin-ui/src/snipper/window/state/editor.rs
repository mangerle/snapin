//! 截图会话的标注编辑状态。

use super::super::annotation_edit::AnnotationDrag;
use crate::editor::annotation_session::AnnotationSession;
use crate::editor::toolbar::Toolbar;
use crate::model::ShapeId;
use snapin_annotation::AnnotationHistory;

/// 工具栏、标注历史、笔迹与文字输入等编辑状态。
pub(crate) struct SnipperEditor {
    /// 共享标注工具栏、笔迹与文字输入会话。
    pub(crate) annotation: AnnotationSession,
    /// 当前截图会话的标注历史。
    pub(crate) annotations: AnnotationHistory,
    /// 当前选中的已提交标注。
    pub(crate) selected_annotation: Option<ShapeId>,
    /// 标注拖动或手柄拉伸会话。
    pub(crate) annotation_drag: Option<AnnotationDrag>,
    /// 记录最近一次鼠标左键点击的时间与坐标，用于精准判定双击交互。
    pub(crate) last_click: Option<(std::time::Instant, (f32, f32))>,
}

impl SnipperEditor {
    /// 使用指定工具栏构建编辑状态。
    pub(crate) fn new(toolbar: Toolbar) -> Self {
        Self {
            annotation: AnnotationSession::new(toolbar),
            annotations: AnnotationHistory::default(),
            selected_annotation: None,
            annotation_drag: None,
            last_click: None,
        }
    }
}
