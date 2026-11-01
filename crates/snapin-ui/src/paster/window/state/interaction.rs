//! 贴图窗口交互状态：工具栏、笔迹、文字会话与右键菜单。
//!
//! 这些状态都由鼠标/键盘事件直接驱动，和贴图内容文档、任务槽、渲染缓存有明确
//! 边界。集中为 PasterInteraction 后，绘制会话的生命周期不再散落在窗口状态中。

use crate::editor::stroke_session::StrokeSession;
use crate::editor::text_input::TextInputSession;
use crate::editor::toolbar::Toolbar;
use crate::paster::context_menu::ContextMenuState;
use std::time::Instant;

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
