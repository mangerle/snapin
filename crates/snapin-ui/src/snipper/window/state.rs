//! 截图交互会话状态机模型与配置初始化。

use super::annotation_edit::AnnotationDrag;
use super::jobs::{SnipperJobKind, SnipperJobRuntime};
use super::ocr_panel::OcrPanelState;
use super::redaction::RedactionPreview;
use super::types::{SnipMode, SnipperOutcome};
use crate::editor::stroke_session::StrokeSession;
use crate::editor::text_input::TextInputSession;
use crate::editor::toolbar::{clear_remembered_sub_options, remembered_sub_options, Toolbar};
use crate::model::ShapeId;
use crate::ports::{DesktopPort, JobCancel, JobId, NoopDesktop, QrCodeItem};
use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{Rect, SelectionConstraint, SelectionHistory, SelectionState};
use crate::snipper::window_rect::WindowRect;
use snapin_core::annotation::AnnotationHistory;
use snapin_core::config::{Config, InterfaceConfig, SnipConfig};
use std::sync::Arc;
use winit::keyboard::ModifiersState;
use winit::window::CursorIcon;

/// 当前在途媒体任务的完整快照。
///
/// 类型、任务 ID 与取消闭包必须同时存在或同时消失；集中为单值可避免三个
/// `Option` 被独立修改后进入“有 ID 无类型”或“有取消闭包无任务”的非法状态。
pub(crate) struct PendingMediaJob {
    kind: SnipperJobKind,
    id: JobId,
    cancel: JobCancel,
}

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
    /// 键盘窗口循环当前索引；鼠标重新悬停后清空
    pub window_cycle: Option<usize>,
    /// 当前选区尺寸约束
    pub selection_constraint: SelectionConstraint,
    pub magnifier: Magnifier,
    pub toolbar: Toolbar,
    pub annotations: AnnotationHistory,
    pub cursor_pos: (f64, f64),
    /// 是否已收到首个光标位置；未就绪时不绘制跟随放大镜，避免左上角出现 (0,0) 占位框
    pub cursor_ready: bool,
    /// 当前窗口光标形状；用于避免鼠标移动时重复提交相同的系统光标设置
    pub cursor_icon: CursorIcon,
    pub modifiers: ModifiersState,
    pub stroke: StrokeSession,
    pub active_text_input: Option<TextInputSession>,
    /// 当前选中的已提交标注在历史栈中的索引
    pub selected_annotation: Option<ShapeId>,
    /// 标注拖动或手柄拉伸会话
    pub annotation_drag: Option<AnnotationDrag>,
    pub outcome: Option<SnipperOutcome>,
    /// 操作失败或空结果提示（显示在画布顶部，下一次操作时清除）
    pub status_message: Option<String>,
    /// 媒体任务运行时（测试或无宿主环境下为空）
    pub job_runtime: Option<Arc<SnipperJobRuntime>>,
    /// 当前在途媒体任务；三要素由单一值维护，避免部分状态组合
    pub(crate) pending_job: Option<PendingMediaJob>,
    /// 智能打码可点击文字区域；马赛克已实时写入标注历史，点击区域切换遮挡
    pub redaction_preview: Option<RedactionPreview>,
    /// 最近一次二维码识别结果；存在时 Ctrl+Enter 可打开首个安全链接
    pub qr_result: Option<Vec<QrCodeItem>>,
    /// OCR 结果面板；存在时按 Shift+O 可重新打开
    pub ocr_panel: Option<OcrPanelState>,
    /// 当前正在编辑的 OCR 面板行
    pub ocr_edit_row: Option<usize>,
    /// 桌面副作用端口：选区缓存、剪贴板等系统交互经此委托给应用层
    pub desktop: Arc<dyn DesktopPort>,
    pub config: Config,
    pub mode: SnipMode,
    /// 首次锁定选区后自动执行的动作；触发后立即清空
    pub post_action: Option<snapin_core::PostCaptureAction>,
}

impl SnipperState {
    pub fn new() -> Self {
        Self::new_with_desktop(Arc::new(NoopDesktop))
    }

    /// 使用宿主注入的桌面端口构建空状态。
    ///
    /// UI 不直接读写系统剪贴板或选区缓存文件，因此单元测试与渲染测试可以使用
    /// [`NoopDesktop`] 构造完全无系统副作用的会话状态。
    pub fn new_with_desktop(desktop: Arc<dyn DesktopPort>) -> Self {
        // 截图工具栏按选区自动摆放，可能遮挡标注区域，因此启用右端拖动手柄
        let mut toolbar = Toolbar::default();
        toolbar.draggable = true;
        Self {
            selection: SelectionState::Idle,
            history: SelectionHistory::default(),
            window_rects: Vec::new(),
            hovered_window: None,
            press_window: None,
            window_cycle: None,
            selection_constraint: SelectionConstraint::Free,
            magnifier: Magnifier::default(),
            toolbar,
            annotations: AnnotationHistory::default(),
            cursor_pos: (0.0, 0.0),
            cursor_ready: false,
            cursor_icon: CursorIcon::Default,
            modifiers: ModifiersState::default(),
            stroke: StrokeSession::new(),
            active_text_input: None,
            selected_annotation: None,
            annotation_drag: None,
            outcome: None,
            status_message: None,
            job_runtime: None,
            pending_job: None,
            redaction_preview: None,
            qr_result: None,
            ocr_panel: None,
            ocr_edit_row: None,
            desktop,
            config: Config::default(),
            mode: SnipMode::default(),
            post_action: None,
        }
    }

    /// 按用户配置决定是否持久化本次选区。
    pub fn remember_selection(&self, rect: Rect) {
        if self.config.snip.remember_last_selection {
            self.desktop.save_last_selection(rect);
        }
    }

    /// 取消当前后台任务（若存在）。
    pub fn cancel_pending_job(&mut self) {
        if let Some(pending) = self.pending_job.take() {
            (pending.cancel)();
        }
    }

    /// 是否存在尚未完成的媒体任务。
    pub fn has_pending_job(&self) -> bool {
        self.pending_job.is_some()
    }

    /// 登记一个已成功提交的媒体任务。
    pub(crate) fn begin_pending_job(&mut self, kind: SnipperJobKind, id: JobId, cancel: JobCancel) {
        debug_assert!(self.pending_job.is_none(), "同一截图会话只能有一个在途任务");
        self.pending_job = Some(PendingMediaJob { kind, id, cancel });
    }

    /// 仅当任务类型与 ID 都匹配时移除在途任务，返回是否为当前任务。
    pub(crate) fn take_matching_job(&mut self, kind: SnipperJobKind, id: JobId) -> bool {
        let matches = self
            .pending_job
            .as_ref()
            .is_some_and(|pending| pending.kind == kind && pending.id == id);
        if matches {
            self.pending_job = None;
        }
        matches
    }

    /// 当前任务 ID；仅供同 crate 的任务状态校验与测试使用。
    #[cfg(test)]
    pub(crate) fn pending_job_id(&self) -> Option<JobId> {
        self.pending_job.as_ref().map(|pending| pending.id)
    }

    /// 设置画布顶部提示信息
    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    /// 清除画布顶部提示信息
    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    /// 根据用户配置、触发模式与宿主端口构建初始状态
    pub fn from_config(cfg: Config, mode: SnipMode, desktop: Arc<dyn DesktopPort>) -> Self {
        let mut state = Self::new_with_desktop(desktop);
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
    state.toolbar.sub_options = crate::editor::toolbar::SubToolOptions::default();
    state.toolbar.sub_options.custom_stroke_width = snip.default_stroke_width;
    state.toolbar.sub_options.font_size = snip.default_font_size;
    state.toolbar.sub_options.color = snip.default_color.to_tiny_skia_color();
    state.toolbar.remember_sub_options = snip.remember_toolbar_settings;
    if snip.remember_toolbar_settings {
        if let Some(options) = remembered_sub_options() {
            state.toolbar.sub_options = options;
        }
    } else {
        clear_remembered_sub_options();
    }
    // 初始进入截图时，未完成选区前不激活标注工具，确保左键优先用于选区拉框
    state.toolbar.active_tool = None;

    // 仅“自定义截屏”复用上次选区；常规截图始终从空白选区开始，
    // 避免启动时自动锁定旧尺寸、妨碍用户重新拉框。
    if snip.remember_last_selection && should_restore_last_selection(mode) {
        if let Some(rect) = state.desktop.load_last_selection() {
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

/// 提交并关闭当前文本输入会话。
///
/// 文本图元同样遵守"最新一条可编辑"的约定：提交后立即把选中切到它，
/// 使刚输入的文字可以马上拖动或用滚轮调字号。
pub fn commit_active_text_input(state: &mut SnipperState) {
    if let Some(row) = state.ocr_edit_row.take() {
        if let Some(session) = state.active_text_input.take() {
            let text = session.finish_text();
            if let Some(panel) = state.ocr_panel.as_mut() {
                panel.replace_line(row, text);
            }
        }
        return;
    }
    if let Some(session) = state.active_text_input.take() {
        if let Some(action) = session.commit() {
            state.annotations.push(action);
            super::annotation_edit::select_latest(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    static TOOLBAR_MEMORY_TEST_LOCK: Mutex<()> = Mutex::new(());

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

    /// 在途任务必须由类型、ID 和取消闭包共同组成，任一项不匹配都不得清除当前任务。
    #[test]
    fn test_pending_job_requires_matching_kind_and_id_and_cancel_runs() {
        let mut state = SnipperState::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_flag = cancelled.clone();
        state.begin_pending_job(
            SnipperJobKind::Ocr,
            JobId::new(7),
            Box::new(move || cancel_flag.store(true, Ordering::Release)),
        );

        assert!(!state.take_matching_job(SnipperJobKind::Save, JobId::new(7)));
        assert!(!state.take_matching_job(SnipperJobKind::Ocr, JobId::new(8)));
        assert!(state.has_pending_job(), "错误回调不得清除当前任务");

        state.cancel_pending_job();
        assert!(cancelled.load(Ordering::Acquire));
        assert!(!state.has_pending_job());
    }

    /// 默认关闭记忆时，新建截图应恢复配置默认值而不是沿用上次改动。
    #[test]
    fn test_toolbar_options_reset_between_captures_by_default() {
        let _guard = TOOLBAR_MEMORY_TEST_LOCK
            .lock()
            .expect("工具栏记忆测试锁不得中毒");
        clear_remembered_sub_options();
        let config = Config::default();
        let desktop: Arc<dyn DesktopPort> = Arc::new(NoopDesktop);
        let mut first =
            SnipperState::from_config(config.clone(), SnipMode::Normal, desktop.clone());
        first.toolbar.sub_options.color = tiny_skia::Color::from_rgba8(0, 0, 255, 255);
        first.toolbar.update_layout(
            &Rect {
                x: 10.0,
                y: 10.0,
                width: 100.0,
                height: 100.0,
            },
            800.0,
            600.0,
        );

        let second = SnipperState::from_config(config, SnipMode::Normal, desktop);
        assert_eq!(
            second.toolbar.sub_options.color,
            tiny_skia::Color::from_rgba8(239, 68, 68, 255)
        );
    }

    /// 开启记忆后，下一次截图应继承上一张截图最后调整的工具栏选项。
    #[test]
    fn test_toolbar_options_can_be_remembered_between_captures() {
        let _guard = TOOLBAR_MEMORY_TEST_LOCK
            .lock()
            .expect("工具栏记忆测试锁不得中毒");
        clear_remembered_sub_options();
        let mut config = Config::default();
        config.snip.remember_toolbar_settings = true;
        let desktop: Arc<dyn DesktopPort> = Arc::new(NoopDesktop);
        let mut first =
            SnipperState::from_config(config.clone(), SnipMode::Normal, desktop.clone());
        first.toolbar.sub_options.color = tiny_skia::Color::from_rgba8(0, 0, 255, 255);
        first.toolbar.update_layout(
            &Rect {
                x: 10.0,
                y: 10.0,
                width: 100.0,
                height: 100.0,
            },
            800.0,
            600.0,
        );

        let second = SnipperState::from_config(config, SnipMode::Normal, desktop);
        assert_eq!(
            second.toolbar.sub_options.color,
            tiny_skia::Color::from_rgba8(0, 0, 255, 255)
        );
        clear_remembered_sub_options();
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
