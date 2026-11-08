//! 录制工具栏拖动与鼠标按压消费状态。

/// 一次工具栏拖动会话的固定起点。
#[derive(Debug, Clone, Copy)]
struct ToolbarDrag {
    pointer: (f32, f32),
    offset: (f32, f32),
}

/// 录制工具栏交互状态。
pub(super) struct ToolbarInteraction {
    offset: (f32, f32),
    drag: Option<ToolbarDrag>,
    mouse_press_consumed: bool,
}

impl ToolbarInteraction {
    /// 创建默认工具栏交互状态。
    pub(super) fn new() -> Self {
        Self {
            offset: (0.0, 0.0),
            drag: None,
            mouse_press_consumed: false,
        }
    }

    /// 工具栏相对默认位置的拖动偏移。
    pub(super) const fn offset(&self) -> (f32, f32) {
        self.offset
    }

    /// 是否正在拖动整条工具栏。
    pub(super) const fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// 开始拖动整条工具栏。
    pub(super) fn begin_drag(&mut self, pointer: (f32, f32)) -> bool {
        if self.drag.is_some() {
            return false;
        }
        self.drag = Some(ToolbarDrag {
            pointer,
            offset: self.offset,
        });
        true
    }

    /// 根据当前鼠标位置更新工具栏偏移。
    pub(super) fn drag_to(&mut self, pointer: (f32, f32)) -> bool {
        let Some(drag) = self.drag else {
            return false;
        };
        let next = (
            drag.offset.0 + pointer.0 - drag.pointer.0,
            drag.offset.1 + pointer.1 - drag.pointer.1,
        );
        if next == self.offset {
            return false;
        }
        self.offset = next;
        true
    }

    /// 结束工具栏拖动。
    pub(super) fn end_drag(&mut self) -> bool {
        self.drag.take().is_some()
    }

    /// 标记当前鼠标按下事件已由录制工具栏消费。
    pub(super) fn consume_mouse_press(&mut self) {
        self.mouse_press_consumed = true;
    }

    /// 读取并清除鼠标按下事件的消费标记。
    pub(super) fn take_mouse_press_consumed(&mut self) -> bool {
        std::mem::take(&mut self.mouse_press_consumed)
    }
}
