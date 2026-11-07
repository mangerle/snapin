//! 截图会话标注图元鼠标按下与选中交互处理。

use super::toolbar_action::handle_text_tool_click;
use crate::editor::toolbar::Tool;
use crate::snipper::window::events::SnipperState;
use snapin_annotation::AnnotationShape;

/// 处理已提交标注的编辑、锁定提示与文字工具入口。
pub(super) fn handle_annotation_press(
    state: &mut SnipperState,
    cx: f32,
    cy: f32,
    is_double_click: bool,
) -> Option<bool> {
    let allow_locked_annotation = state.viewport.modifiers.alt_key();

    // 0. 双击优先唤起文字就地编辑
    if is_double_click {
        let hit_index = state
            .editor
            .selected_annotation
            .map(|id| id.get())
            .filter(|&idx| {
                state
                    .editor
                    .annotations
                    .bounds_at(idx)
                    .is_some_and(|b| b.contains(cx, cy))
            })
            .or_else(|| {
                crate::snipper::window::annotation_edit::hit_test_editable(
                    state,
                    (cx, cy),
                    allow_locked_annotation,
                )
            });

        if let Some(index) = hit_index {
            if crate::snipper::window::annotation_edit::start_editing_text_annotation(state, index)
            {
                return Some(true);
            }
        }
    }

    // 1. 如果用户显式切换并激活了文字工具（Tool::Text）：
    if state.editor.annotation.toolbar.active_tool == Some(Tool::Text) && !allow_locked_annotation {
        // 如果点击命中了已存在的文字标注，允许选中并拖动移动它；
        // 若需修改文字内容，在文字上方双击或按 Enter 即可进入重新编辑。
        if let Some(index) = crate::snipper::window::annotation_edit::hit_test_editable(
            state,
            (cx, cy),
            allow_locked_annotation,
        ) {
            if matches!(
                state.editor.annotations.shape_at(index),
                Some(AnnotationShape::Text(_))
            ) && crate::snipper::window::annotation_edit::handle_press(
                state,
                cx,
                cy,
                allow_locked_annotation,
            ) {
                return Some(true);
            }
        }

        crate::snipper::window::annotation_edit::clear(state);
        return Some(handle_text_tool_click(state, cx, cy));
    }

    // 2. 如果命中了已选中标注的手柄，用户明确意图是缩放微调，优先响应手柄调整
    if crate::snipper::window::annotation_edit::hit_test_selected_handle(state, cx, cy).is_some()
        && crate::snipper::window::annotation_edit::handle_press(state, cx, cy, false)
    {
        return Some(true);
    }

    // 3. 在未激活工具（选择模式）或按住 Alt 时，允许选中并拖动已有标注
    if crate::snipper::window::annotation_edit::handle_press(state, cx, cy, allow_locked_annotation)
    {
        return Some(true);
    }

    // 4. 未激活绘制工具时点击旧标注：给出 Alt 解锁提示
    if state.editor.annotation.toolbar.active_tool.is_none()
        && crate::snipper::window::annotation_edit::hit_test_locked(state, (cx, cy))
    {
        state.set_status_message(snapin_foundation::i18n::tr(
            "旧标注已锁定，按住 Alt 点击可重新编辑",
        ));
        return Some(true);
    }

    None
}
