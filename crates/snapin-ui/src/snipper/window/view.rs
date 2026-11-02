//! 可在宿主事件循环中托管的截图覆盖层视图。
//!
//! # 设计原理
//!
//! winit 每进程只允许创建一个 `EventLoop`。常驻守护进程已持有贴图工作区事件循环，
//! 若截图仍自建事件循环，就只能为每次截图派生新进程。本视图把窗口、呈现器与交互
//! 状态封装成一个可插入现有事件循环的组件：单次截图会话与常驻工作区都能复用它，
//! 分别决定结果回调与事件循环归属。
//!
//! 视图不持有事件循环、不决定退出时机；宿主在每次事件后检查 [`SnipperView::take_outcome`]。

use super::event_dispatcher::{SnipperDispatchContext, SnipperEventDispatcher};
use super::events::SnipperState;
use super::jobs::{handle_job_outcome, SnipperJobKind, SnipperJobRuntime, SnipperJobSink};
use super::presenter::{SnipperPresenter, SnipperPresenterInit};
use super::state::clamp_window_rect;
use super::types::{SnipMode, SnipperLaunch, SnipperOutcome, SnipperRuntime};
use crate::error::UiError;
use crate::ports::{ClipboardEventSink, ClipboardJobId, ClipboardResult, PortError};
use crate::snipper::selection::{Rect, SelectionState};
use image::RgbaImage;
use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window, WindowLevel};

/// 全屏截图覆盖层视图。
pub struct SnipperView {
    window: Arc<Window>,
    presenter: SnipperPresenter,
    state: SnipperState,
    bg_image: RgbaImage,
}

impl SnipperView {
    /// 在既有事件循环中创建截图覆盖层。
    ///
    /// # Errors
    ///
    /// 窗口或软件帧缓冲创建失败时返回 [`UiError::Window`] / [`UiError::Softbuffer`]。
    pub fn new(
        window_target: &ActiveEventLoop,
        launch: SnipperLaunch,
        job_sink: SnipperJobSink,
        clipboard_events: ClipboardEventSink,
    ) -> Result<Self, UiError> {
        let SnipperLaunch {
            bg_image,
            windows,
            runtime,
        } = launch;
        let screen_w = bg_image.width();
        let screen_h = bg_image.height();
        let window = create_overlay_window(window_target, screen_w, screen_h)?;
        let mode = runtime.mode;
        let presenter = SnipperPresenter::try_new(SnipperPresenterInit {
            window: window.clone(),
            width: screen_w,
            height: screen_h,
            bg_image: &bg_image,
            dimmed: mode != SnipMode::PickColor,
        })?;
        let mut state = build_initial_state(SnipperStateInit {
            runtime,
            windows,
            mode,
            screen_size: (screen_w, screen_h),
            job_sink,
            clipboard_events,
        });
        apply_ui_scale(&window, &mut state, screen_w, screen_h);

        window.request_redraw();
        Ok(Self {
            window,
            presenter,
            state,
            bg_image,
        })
    }
}

/// 创建覆盖层全屏窗口。
fn create_overlay_window(
    window_target: &ActiveEventLoop,
    screen_w: u32,
    screen_h: u32,
) -> Result<Arc<Window>, UiError> {
    let attributes = Window::default_attributes()
        .with_title("Snapin")
        .with_decorations(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_fullscreen(Some(Fullscreen::Borderless(None)))
        .with_inner_size(PhysicalSize::new(screen_w, screen_h));
    let window = window_target
        .create_window(attributes)
        .map_err(|error| UiError::Window {
            message: format!("创建全屏截图窗口失败: {error}"),
        })?;
    window.set_ime_allowed(true);
    Ok(Arc::new(window))
}

/// 截图交互状态初始化参数。
struct SnipperStateInit {
    runtime: SnipperRuntime,
    windows: Vec<crate::snipper::WindowRect>,
    mode: SnipMode,
    screen_size: (u32, u32),
    job_sink: SnipperJobSink,
    clipboard_events: ClipboardEventSink,
}

/// 根据启动参数构建截图交互状态。
fn build_initial_state(init: SnipperStateInit) -> SnipperState {
    let SnipperStateInit {
        runtime,
        windows,
        mode,
        screen_size: (screen_w, screen_h),
        job_sink,
        clipboard_events,
    } = init;
    let mut state = SnipperState::from_config(runtime.config, mode, runtime.desktop);
    state.session.post_action = runtime.post_action;
    state.viewport.window_rects = windows
        .iter()
        .filter_map(|window| clamp_window_rect(*window, screen_w as f32, screen_h as f32))
        .collect();
    if mode == SnipMode::Fullscreen {
        state.viewport.selection = SelectionState::Locked {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w as f32,
                height: screen_h as f32,
            },
            active_drag: None,
        };
    } else if mode == SnipMode::PickColor {
        state.viewport.magnifier.enabled = true;
        state.set_status_message(snapin_foundation::i18n::tr("单击复制颜色，Esc 取消"));
    }

    state.jobs.runtime = Some(Arc::new(SnipperJobRuntime::new(runtime.media, job_sink)));
    state.jobs.clipboard_events = clipboard_events;
    state
}

/// 按窗口实际缩放因子与屏幕尺寸初始化工具栏和放大镜缩放。
fn apply_ui_scale(window: &Window, state: &mut SnipperState, screen_w: u32, screen_h: u32) {
    let raw_scale = window.scale_factor() as f32;
    let auto_scale = if raw_scale > 1.05 {
        raw_scale
    } else if screen_w >= 2560 {
        2.0
    } else {
        1.0
    } * state.session.config.interface.toolbar_scale;
    state.editor.annotation.toolbar.ui_scale = auto_scale;
    state.viewport.magnifier.ui_scale = auto_scale;

    if let SelectionState::Locked { rect, .. } = &state.viewport.selection {
        state
            .editor
            .annotation
            .toolbar
            .update_layout(rect, screen_w as f32, screen_h as f32);
    }
}

impl SnipperView {
    /// 覆盖层窗口标识，宿主据此路由窗口事件。
    pub fn window_id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    /// 处理一个属于本窗口的 winit 事件。
    pub fn dispatch(&mut self, event: WindowEvent) {
        SnipperEventDispatcher::dispatch(
            event,
            SnipperDispatchContext {
                window: self.window.as_ref(),
                presenter: &mut self.presenter,
                state: &mut self.state,
                bg_image: &self.bg_image,
            },
        );
    }

    /// 处理媒体任务完成事件。
    pub fn handle_job_finished(
        &mut self,
        job_id: crate::ports::JobId,
        kind: SnipperJobKind,
        outcome: crate::ports::MediaJobOutcome,
    ) {
        handle_job_outcome(&mut self.state, job_id, kind, outcome);
    }

    /// 处理剪贴板写入完成事件。
    ///
    /// 只有真实写入成功后才设置最终结果并触发覆盖层关闭；失败时保持窗口打开并
    /// 展示可操作的错误提示。
    pub fn handle_clipboard_finished(
        &mut self,
        job_id: ClipboardJobId,
        result: Result<ClipboardResult, PortError>,
    ) {
        self.state.handle_clipboard_finished(job_id, result);
    }

    /// 取出会话结果；存在结果时会取消仍在执行的后台任务。
    pub fn take_outcome(&mut self) -> Option<SnipperOutcome> {
        let outcome = self.state.session.outcome.take();
        if outcome.is_some() {
            self.state.cancel_pending_job();
        }
        outcome
    }

    /// 请求重绘。
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// 事件循环退出前取消后台任务并关闭 IME。
    pub fn cancel(&mut self) {
        self.state.cancel_pending_job();
        self.state.jobs.pending_clipboard = None;
        self.window.set_ime_allowed(false);
    }
}
