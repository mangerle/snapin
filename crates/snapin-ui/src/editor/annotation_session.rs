//! 截图与贴图共享的标注交互会话。
//!
//! 工具栏、当前笔迹和文字输入是两侧完全一致的交互状态。将其收敛为同一个类型，
//! 避免截图与贴图各自维护一套字段和生命周期规则；标注历史、图像写入、选择与拖拽
//! 等差异仍由各自窗口状态负责。

use crate::editor::stroke_session::StrokeSession;
use crate::editor::text_input::TextInputSession;
use crate::editor::toolbar::Toolbar;

/// 当前标注工具的交互会话。
pub(crate) struct AnnotationSession {
    /// 当前工具栏与样式状态。
    pub(crate) toolbar: Toolbar,
    /// 当前一笔标注草稿。
    pub(crate) stroke: StrokeSession,
    /// 当前文字输入会话。
    pub(crate) active_text_input: Option<TextInputSession>,
    /// 是否正处于鼠标拖拽选中文本状态。
    pub(crate) is_selecting_text: bool,
}

impl AnnotationSession {
    /// 使用指定工具栏创建交互会话。
    pub(crate) fn new(toolbar: Toolbar) -> Self {
        Self {
            toolbar,
            stroke: StrokeSession::new(),
            active_text_input: None,
            is_selecting_text: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 共享会话初始化后必须处于空闲且已预分配笔迹容量。
    #[test]
    fn test_annotation_session_starts_idle() {
        let session = AnnotationSession::new(Toolbar::default());
        assert!(session.active_text_input.is_none());
        assert!(!session.stroke.is_drawing);
        assert!(session.stroke.points.capacity() >= 64);
    }
}
