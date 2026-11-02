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

use super::event_dispatcher::SnipperEventDispatcher;
use super::events::SnipperState;
use super::jobs::{handle_job_outcome, SnipperJobKind, SnipperJobRuntime, SnipperJobSink};
use super::presenter::SnipperPresenter;
use super::state::clamp_window_rect;
use super::types::{SnipMode, SnipperLaunch, SnipperOutcome, SnipperRuntime};
use crate::error::UiError;
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
        let presenter = SnipperPresenter::try_new(
            window.clone(),
            screen_w,
            screen_h,
            &bg_image,
            mode != SnipMode::PickColor,
        )?;
        let mut state = build_initial_state(runtime, windows, mode, screen_w, screen_h, job_sink);
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

/// 根据启动参数构建截图交互状态。
fn build_initial_state(
    runtime: SnipperRuntime,
    windows: Vec<crate::snipper::WindowRect>,
    mode: SnipMode,
    screen_w: u32,
    screen_h: u32,
    job_sink: SnipperJobSink,
) -> SnipperState {
    let mut state = SnipperState::from_config(runtime.config, mode, runtime.desktop);
    state.post_action = runtime.post_action;
    state.window_rects = windows
        .iter()
        .filter_map(|window| clamp_window_rect(*window, screen_w as f32, screen_h as f32))
        .collect();
    if mode == SnipMode::Fullscreen {
        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w as f32,
                height: screen_h as f32,
            },
            active_drag: None,
        };
    } else if mode == SnipMode::PickColor {
        state.magnifier.enabled = true;
        state.set_status_message(snapin_foundation::i18n::tr("单击复制颜色，Esc 取消"));
    }

    state.job_runtime = Some(Arc::new(SnipperJobRuntime::new(runtime.media, job_sink)));
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
    } * state.config.interface.toolbar_scale;
    state.toolbar.ui_scale = auto_scale;
    state.magnifier.ui_scale = auto_scale;

    if let SelectionState::Locked { rect, .. } = &state.selection {
        state
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
            &self.window,
            &mut self.presenter,
            &mut self.state,
            &self.bg_image,
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

    /// 取出会话结果；存在结果时会取消仍在执行的后台任务。
    pub fn take_outcome(&mut self) -> Option<SnipperOutcome> {
        let outcome = self.state.outcome.take();
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
        self.window.set_ime_allowed(false);
    }
}
