//! 贴图窗口交互状态：工具栏、笔迹、文字会话与右键菜单。
//!
//! 这些状态都由鼠标/键盘事件直接驱动，和贴图内容文档、任务槽、渲染缓存有明确
//! 边界。集中为 PasterInteraction 后，绘制会话的生命周期不再散落在窗口状态中。

use crate::editor::stroke_session::StrokeSession;
use crate::editor::text_input::TextInputSession;
use crate::editor::toolbar::Toolbar;
use crate::paster::context_menu::ContextMenuState;
use std::time::Instant;

/// 贴图窗口当前交互模式。
///
/// 该枚举是笔迹、文字会话与右键菜单的派生视图，用于事件分发时避免到处组合
/// `active_text_input`、`is_drawing`、`context_menu.is_open` 三个布尔条件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InteractionMode {
    /// 空闲
    Idle,
    /// 正在绘制一笔
    Drawing,
    /// 正在输入文字
    TextInput,
    /// 右键菜单展开
    Menu,
}

/// 贴图窗口交互状态。
pub(crate) struct PasterInteraction {
    /// 标注工具栏
    pub(crate) toolbar: Toolbar,
    /// 当前一笔标注草稿（图像像素坐标）
    pub(crate) stroke: StrokeSession,
    /// 正在编辑的文字标注会话
    pub(crate) active_text_input: Option<TextInputSession>,
    /// 右键上下文菜单
    pub(crate) context_menu: ContextMenuState,
    /// 上一次左键点击时刻，用于双击检测
    pub(crate) last_click_time: Option<Instant>,
}

impl PasterInteraction {
    /// 派生当前交互模式。
    pub(crate) fn mode(&self) -> InteractionMode {
        if self.active_text_input.is_some() {
            InteractionMode::TextInput
        } else if self.stroke.is_drawing {
            InteractionMode::Drawing
        } else if self.context_menu.is_open {
            InteractionMode::Menu
        } else {
            InteractionMode::Idle
        }
    }

    /// 用指定工具栏创建交互状态。
    pub(crate) fn new(toolbar: Toolbar) -> Self {
        Self {
            toolbar,
            stroke: StrokeSession::new(),
            active_text_input: None,
            context_menu: ContextMenuState::default(),
            last_click_time: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::toolbar::Toolbar;

    /// 交互模式是状态的派生视图，优先级为文字 > 绘制 > 菜单 > 空闲。
    #[test]
    fn test_interaction_mode_projection() {
        let mut interaction = PasterInteraction::new(Toolbar::default());
        assert_eq!(interaction.mode(), InteractionMode::Idle);

        interaction.context_menu.is_open = true;
        assert_eq!(interaction.mode(), InteractionMode::Menu);

        interaction.stroke.is_drawing = true;
        assert_eq!(interaction.mode(), InteractionMode::Drawing);

        interaction.active_text_input = Some(TextInputSession::new(
            (0.0, 0.0),
            (255, 255, 255, 255),
            16.0,
        ));
        assert_eq!(interaction.mode(), InteractionMode::TextInput);
    }
}
