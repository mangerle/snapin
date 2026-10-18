//! 标注图元命令模式与撤销/重做控制。
//!
//! 将每一次绘制操作封装为具备 Execute 与 Undo 能力的 Command 对象，
//! 采用命令模式维护 Undo/Redo 历史栈。

use tiny_skia::PixmapMut;

/// 单次矢量标注行为的 Trait 抽象
pub trait AnnotationAction: Send + Sync {
    /// 执行绘制
    fn draw(&self, pixmap: &mut PixmapMut);
}

/// 标注历史管理器 (Undo / Redo 栈)
#[derive(Default)]
pub struct AnnotationHistory {
    undo_stack: Vec<Box<dyn AnnotationAction>>,
    redo_stack: Vec<Box<dyn AnnotationAction>>,
}

impl AnnotationHistory {
    /// 添加一条新的标注操作，清空 redo 栈
    pub fn push(&mut self, action: Box<dyn AnnotationAction>) {
        self.undo_stack.push(action);
        self.redo_stack.clear();
    }

    /// 撤销上一次标注
    pub fn undo(&mut self) -> bool {
        if let Some(action) = self.undo_stack.pop() {
            self.redo_stack.push(action);
            true
        } else {
            false
        }
    }

    /// 恢复被撤销的标注
    pub fn redo(&mut self) -> bool {
        if let Some(action) = self.redo_stack.pop() {
            self.undo_stack.push(action);
            true
        } else {
            false
        }
    }

    /// 重新在目标 pixmap 上依次回放所有生效的标注图元
    pub fn replay_all(&self, pixmap: &mut PixmapMut) {
        for action in &self.undo_stack {
            action.draw(pixmap);
        }
    }

    /// 当前是否有可撤销的操作
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// 当前是否有可重做的操作
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// 清空所有标注历史
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyAction;
    impl AnnotationAction for DummyAction {
        fn draw(&self, _pixmap: &mut PixmapMut) {}
    }

    #[test]
    fn test_undo_redo_stack() {
        let mut history = AnnotationHistory::default();
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        history.push(Box::new(DummyAction));
        assert!(history.can_undo());
        assert!(!history.can_redo());

        assert!(history.undo());
        assert!(!history.can_undo());
        assert!(history.can_redo());

        assert!(history.redo());
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }
}
