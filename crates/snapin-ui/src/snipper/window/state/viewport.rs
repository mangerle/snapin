//! 截图视口、选区与光标展示状态。

use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{Rect, SelectionConstraint, SelectionHistory, SelectionState};
use winit::keyboard::ModifiersState;
use winit::window::CursorIcon;

/// 截图覆盖层的视口与选区状态。
pub(crate) struct SnipperViewport {
    /// 当前选区状态机。
    pub(crate) selection: SelectionState,
    /// 历史选区回溯栈。
    pub(crate) history: SelectionHistory,
    /// 窗口截图候选矩形（物理像素，按堆叠顺序从底到顶）。
    pub(crate) window_rects: Vec<Rect>,
    /// 光标当前悬停的窗口矩形。
    pub(crate) hovered_window: Option<Rect>,
    /// 本次按下时悬停的窗口；短按未拖动时吸附到该窗口。
    pub(crate) press_window: Option<Rect>,
    /// 键盘窗口循环当前索引；鼠标重新悬停后清空。
    pub(crate) window_cycle: Option<usize>,
    /// 当前选区尺寸约束。
    pub(crate) selection_constraint: SelectionConstraint,
    /// 放大镜与取色显示状态。
    pub(crate) magnifier: Magnifier,
    /// 光标屏幕坐标。
    pub(crate) cursor_pos: (f64, f64),
    /// 是否已收到首个光标位置。
    pub(crate) cursor_ready: bool,
    /// 当前窗口光标形状。
    pub(crate) cursor_icon: CursorIcon,
    /// 当前修饰键状态。
    pub(crate) modifiers: ModifiersState,
    /// 当前覆盖层物理像素尺寸。
    pub(crate) screen_size: (f32, f32),
}

impl SnipperViewport {
    /// 构建空闲选区视口。
    pub(crate) fn new() -> Self {
        Self {
            selection: SelectionState::Idle,
            history: SelectionHistory::default(),
            window_rects: Vec::new(),
            hovered_window: None,
            press_window: None,
            window_cycle: None,
            selection_constraint: SelectionConstraint::Free,
            magnifier: Magnifier::default(),
            cursor_pos: (0.0, 0.0),
            cursor_ready: false,
            cursor_icon: CursorIcon::Default,
            modifiers: ModifiersState::default(),
            screen_size: (0.0, 0.0),
        }
    }
}
