//! 截图交互会话显式阶段状态机模型。

use super::SnipperState;
use crate::snipper::selection::SelectionState;

/// 截图会话当前的高层交互阶段。
///
/// 截图覆盖层具备互斥的生命周期状态。通过聚合各子模块的低层瞬态推导出单一的
/// 代数状态，使渲染编排、光标决策与事件拦截在类型层面清晰可见，消除依靠多个
/// Option 与布尔值手工拼装判断条件的隐式状态推断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnipperPhase {
    /// 选区未建立，光标自由悬停，可探测窗口与放大镜取色
    Idle,
    /// 正在鼠标拖拽拉框创建初始选区
    CreatingSelection,
    /// 选区已建立并锁定，工具栏已展示，处于待命交互状态
    SelectionSettled,
    /// 正在拖拽移动选区或拉伸选区 8 个控制手柄
    AdjustingSelection,
    /// 正在绘制矢量标注草稿（线条/几何图形/画笔等）
    DrawingStroke,
    /// 正在拖拽或拉伸已提交的矢量标注图元
    DraggingAnnotation,
    /// 正在实时文字输入框中打字编辑
    TextAnnotating,
    /// 文字识别主按钮的右键上下文菜单已弹出
    OcrMenu,
    /// 独立多行 OCR 文本编辑器正在展示并编辑文字
    OcrEditing,
    /// 后台媒体异步任务（OCR推理、翻译网络请求、图像保存等）执行中
    Busy,
    /// 选区录屏工具栏正在工作
    Recording,
    /// 正在进行长截图录制捕获或停止待命
    LongCapturing,
    /// 长截图拼接完成，正在进行全功能长图滚动标注编辑
    LongCaptureEditing,
}

impl SnipperPhase {
    /// 当前阶段是否属于已有锁定选区后的工作状态。
    pub fn has_selection(self) -> bool {
        !matches!(self, Self::Idle | Self::CreatingSelection)
    }

    /// 当前阶段是否正在进行鼠标拖拽操作。
    pub fn is_dragging(self) -> bool {
        matches!(
            self,
            Self::CreatingSelection
                | Self::AdjustingSelection
                | Self::DrawingStroke
                | Self::DraggingAnnotation
        )
    }

    /// 当前阶段是否正在阻塞等待异步任务完成。
    pub fn is_busy(self) -> bool {
        matches!(self, Self::Busy)
    }

    /// 当前是否处于长截图流程（正在采集、已暂停长截图或进入长图编辑态）。
    pub fn is_long_capture(self) -> bool {
        matches!(self, Self::LongCapturing | Self::LongCaptureEditing)
    }
}

impl SnipperState {
    /// 推导当前会话所处的高层交互阶段。
    pub fn phase(&self) -> SnipperPhase {
        if let Some(session) = &self.long_capture {
            if session.is_editing {
                return SnipperPhase::LongCaptureEditing;
            }
            return SnipperPhase::LongCapturing;
        }
        if self.recording.is_some() {
            return SnipperPhase::Recording;
        }
        if self.ocr.editor.is_some() {
            return SnipperPhase::OcrEditing;
        }
        if self.ocr.menu.is_some() {
            return SnipperPhase::OcrMenu;
        }
        if self.jobs.pending_media.is_some() {
            return SnipperPhase::Busy;
        }
        if self.editor.annotation.active_text_input.is_some() {
            return SnipperPhase::TextAnnotating;
        }
        if self.editor.annotation.stroke.is_drawing {
            return SnipperPhase::DrawingStroke;
        }
        if self.editor.annotation_drag.is_some() {
            return SnipperPhase::DraggingAnnotation;
        }
        match self.viewport.selection {
            SelectionState::Idle => SnipperPhase::Idle,
            SelectionState::Creating { .. } => SnipperPhase::CreatingSelection,
            SelectionState::Locked {
                active_drag: Some(_),
                ..
            } => SnipperPhase::AdjustingSelection,
            SelectionState::Locked {
                active_drag: None, ..
            } => SnipperPhase::SelectionSettled,
        }
    }

    /// 当前阶段是否允许拖拽边界手柄、整体移动或使用方向键微调锁定选区。
    ///
    /// 长截图仅在“已暂停且未进入长图编辑态”时允许重新调整选区；正在录制或处于
    /// 长图编辑态时必须锁定选区几何尺寸，防止破坏当前已拼接或待标注的画布坐标系。
    pub(crate) fn can_adjust_selection(&self) -> bool {
        !self
            .long_capture
            .as_ref()
            .is_some_and(|session| session.is_capturing || session.is_editing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::toolbar::Tool;
    use crate::geometry::Rect;
    use crate::snipper::selection::DragMode;

    #[test]
    fn test_phase_idle_and_creating() {
        let mut state = SnipperState::new();
        assert_eq!(state.phase(), SnipperPhase::Idle);
        assert!(!state.phase().has_selection());

        state.viewport.selection = SelectionState::Creating {
            start: (10.0, 10.0),
            current: (100.0, 100.0),
        };
        assert_eq!(state.phase(), SnipperPhase::CreatingSelection);
        assert!(state.phase().is_dragging());
    }

    #[test]
    fn test_phase_locked_and_adjusting() {
        let mut state = SnipperState::new();
        let rect = Rect::new(50.0, 50.0, 200.0, 150.0);
        state.viewport.selection = SelectionState::Locked {
            rect,
            active_drag: None,
        };
        assert_eq!(state.phase(), SnipperPhase::SelectionSettled);
        assert!(state.phase().has_selection());
        assert!(!state.phase().is_dragging());

        state.viewport.selection = SelectionState::Locked {
            rect,
            active_drag: Some(DragMode::Move {
                offset_x: 5,
                offset_y: 5,
            }),
        };
        assert_eq!(state.phase(), SnipperPhase::AdjustingSelection);
        assert!(state.phase().is_dragging());
    }

    #[test]
    fn test_phase_drawing_stroke_takes_precedence_over_settled() {
        let mut state = SnipperState::new();
        state.viewport.selection = SelectionState::Locked {
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            active_drag: None,
        };
        state.editor.annotation.toolbar.active_tool = Some(Tool::Pen);
        state.editor.annotation.stroke.is_drawing = true;
        assert_eq!(state.phase(), SnipperPhase::DrawingStroke);
        assert!(state.phase().is_dragging());
    }
}
