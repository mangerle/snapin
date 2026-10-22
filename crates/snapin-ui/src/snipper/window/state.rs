//! 截图交互会话状态机模型与配置初始化。

use super::jobs::{SnipperJobDispatcher, SnipperJobKind};
use super::text_input::TextInputSession;
use super::types::{SnipMode, SnipperOutcome};
use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{Rect, SelectionHistory, SelectionState};
use crate::snipper::toolbar::Toolbar;
use snapin_core::annotation::AnnotationHistory;
use snapin_core::config::{Config, InterfaceConfig, SnipConfig};
use snapin_platform::WindowRect;
use std::sync::Arc;
use winit::keyboard::ModifiersState;

/// 截图会话可变运行状态封装
pub struct SnipperState {
    pub selection: SelectionState,
    pub history: SelectionHistory,
    /// 窗口截图候选矩形（物理像素，按堆叠顺序从底到顶）
    pub window_rects: Vec<Rect>,
    /// 光标当前悬停的窗口矩形
    pub hovered_window: Option<Rect>,
    /// 本次按下时悬停的窗口；短按未拖动时吸附到该窗口
    pub press_window: Option<Rect>,
    pub magnifier: Magnifier,
    pub toolbar: Toolbar,
    pub annotations: AnnotationHistory,
    pub cursor_pos: (f64, f64),
    /// 是否已收到首个光标位置；未就绪时不绘制跟随放大镜，避免左上角出现 (0,0) 占位框
    pub cursor_ready: bool,
    pub modifiers: ModifiersState,
    pub pending_stroke_points: Vec<(f32, f32)>,
    pub active_text_input: Option<TextInputSession>,
    pub outcome: Option<SnipperOutcome>,
    /// 操作失败或空结果提示（显示在画布顶部，下一次操作时清除）
    pub status_message: Option<String>,
    /// 后台任务调度器（测试或非异步环境下为空）
    pub job_dispatcher: Option<Arc<SnipperJobDispatcher>>,
    /// 当前正在执行的 OCR/翻译任务类型
    pub pending_job: Option<SnipperJobKind>,
    /// 当前后台任务的取消句柄
    pub pending_job_abort: Option<tokio::task::AbortHandle>,
    pub config: Config,
    pub mode: SnipMode,
}

impl SnipperState {
    pub fn new() -> Self {
        Self {
            selection: SelectionState::Idle,
            history: SelectionHistory::default(),
            window_rects: Vec::new(),
            hovered_window: None,
            press_window: None,
            magnifier: Magnifier::default(),
            toolbar: Toolbar::default(),
            annotations: AnnotationHistory::default(),
            cursor_pos: (0.0, 0.0),
            cursor_ready: false,
            modifiers: ModifiersState::default(),
            pending_stroke_points: Vec::with_capacity(64),
            active_text_input: None,
            outcome: None,
            status_message: None,
            job_dispatcher: None,
            pending_job: None,
            pending_job_abort: None,
            config: Config::default(),
            mode: SnipMode::default(),
        }
    }

    /// 按用户配置决定是否持久化本次选区。
    pub fn remember_selection(&self, rect: Rect) {
        if self.config.snip.remember_last_selection {
            snapin_platform::save_last_selection((rect.x, rect.y, rect.width, rect.height));
        }
    }

    /// 取消当前后台任务（若存在）。
    pub fn cancel_pending_job(&mut self) {
        if let Some(abort) = self.pending_job_abort.take() {
            abort.abort();
        }
        self.pending_job = None;
    }

    /// 设置画布顶部提示信息
    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    /// 清除画布顶部提示信息
    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    /// 根据用户配置文件与触发模式构建初始状态
    pub fn from_config(cfg: Config, mode: SnipMode) -> Self {
        let mut state = Self::new();
        apply_interface_config(&mut state, &cfg.interface);
        apply_snip_config(&mut state, &cfg.snip, mode);
        state.config = cfg;
        state.mode = mode;
        state
    }
}

impl Default for SnipperState {
    fn default() -> Self {
        Self::new()
    }
}

/// 注入界面视觉与放大镜设置
fn apply_interface_config(state: &mut SnipperState, iface: &InterfaceConfig) {
    state.magnifier.enabled = iface.enable_magnifier;
    state.magnifier.scale = iface.magnifier_scale;
    state.magnifier.show_grid = iface.show_grid_lines;
    state.magnifier.format = iface.color_format;
    state.toolbar.ui_scale = iface.toolbar_scale;
}

/// 注入截图流程、默认标注工具与选区记忆设置
fn apply_snip_config(state: &mut SnipperState, snip: &SnipConfig, mode: SnipMode) {
    state.toolbar.sub_options.custom_stroke_width = snip.default_stroke_width;
    state.toolbar.sub_options.font_size = snip.default_font_size;
    state.toolbar.sub_options.color = snip.default_color.to_tiny_skia_color();
    // 初始进入截图时，未完成选区前不激活标注工具，确保左键优先用于选区拉框
    state.toolbar.active_tool = None;

    // 仅“自定义截屏”复用上次选区；常规截图始终从空白选区开始，
    // 避免启动时自动锁定旧尺寸、妨碍用户重新拉框。
    if snip.remember_last_selection && should_restore_last_selection(mode) {
        if let Some((lx, ly, lw, lh)) = snapin_platform::load_last_selection() {
            let rect = crate::snipper::selection::Rect {
                x: lx,
                y: ly,
                width: lw,
                height: lh,
            };
            state.selection = SelectionState::Locked {
                rect,
                active_drag: None,
            };
        }
    }
}

/// 判断当前触发模式是否允许恢复历史选区。
fn should_restore_last_selection(mode: SnipMode) -> bool {
    mode == SnipMode::Custom
}

/// 将扩展返回的窗口矩形夹取到截图图像范围内。
pub(crate) fn clamp_window_rect(win: WindowRect, screen_w: f32, screen_h: f32) -> Option<Rect> {
    let left = (win.x as f32).max(0.0).min(screen_w);
    let top = (win.y as f32).max(0.0).min(screen_h);
    let right = (win.x as f32 + win.width as f32).max(0.0).min(screen_w);
    let bottom = (win.y as f32 + win.height as f32).max(0.0).min(screen_h);
    let width = (right - left).max(0.0);
    let height = (bottom - top).max(0.0);
    if width > 1.0 && height > 1.0 {
        Some(Rect {
            x: left,
            y: top,
            width,
            height,
        })
    } else {
        None
    }
}

/// 查找光标位置命中的顶层窗口；窗口列表按从底到顶排列。
pub(crate) fn window_at(pos: (f32, f32), windows: &[Rect]) -> Option<Rect> {
    windows
        .iter()
        .rev()
        .find(|rect| rect.contains(pos.0, pos.1))
        .copied()
}

/// 提交并关闭当前文本输入会话
pub fn commit_active_text_input(state: &mut SnipperState) {
    if let Some(session) = state.active_text_input.take() {
        if let Some(action) = session.commit() {
            state.annotations.push(action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 仅自定义截图恢复历史选区，常规截图始终从空白起步
    #[test]
    fn test_should_restore_last_selection_by_mode() {
        assert!(!should_restore_last_selection(SnipMode::Normal));
        assert!(should_restore_last_selection(SnipMode::Custom));
        assert!(!should_restore_last_selection(SnipMode::AutoCopy));
        assert!(!should_restore_last_selection(SnipMode::Translate));
        assert!(!should_restore_last_selection(SnipMode::Fullscreen));
        assert!(!should_restore_last_selection(SnipMode::PickColor));
    }

    /// 初始状态不得把 (0,0) 当成真实光标位置
    #[test]
    fn test_cursor_not_ready_until_first_move() {
        assert!(!SnipperState::new().cursor_ready);
    }

    /// 窗口矩形应夹取到截图范围，完全越界的窗口被丢弃
    #[test]
    fn test_clamp_window_rect_clips_to_screen() {
        let rect = clamp_window_rect(
            WindowRect {
                x: -10,
                y: 20,
                width: 200,
                height: 100,
            },
            100.0,
            80.0,
        )
        .expect("部分可见窗口应保留");
        assert_eq!(
            rect,
            Rect {
                x: 0.0,
                y: 20.0,
                width: 100.0,
                height: 60.0,
            }
        );

        assert!(
            clamp_window_rect(
                WindowRect {
                    x: 200,
                    y: 200,
                    width: 10,
                    height: 10,
                },
                100.0,
                80.0,
            )
            .is_none(),
            "完全越界窗口必须丢弃"
        );
    }

    /// 悬停命中应优先选择堆叠列表中更靠上的窗口
    #[test]
    fn test_window_at_prefers_topmost() {
        let bottom = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let top = Rect {
            x: 20.0,
            y: 20.0,
            width: 50.0,
            height: 50.0,
        };
        assert_eq!(window_at((30.0, 30.0), &[bottom, top]), Some(top));
        assert_eq!(window_at((5.0, 5.0), &[bottom, top]), Some(bottom));
        assert_eq!(window_at((200.0, 200.0), &[bottom, top]), None);
    }
}
