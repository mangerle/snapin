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
use super::state::{clamp_window_rect, CapturedFrameEffect, RecordingTransition};
use super::types::{SnipMode, SnipperLaunch, SnipperOutcome, SnipperRuntime};
use crate::error::UiError;
use crate::ports::{
    ClipboardEventSink, ClipboardJobId, ClipboardResult, PortError, RecordingEvent,
    RecordingEventSink, ScreenCaptureEventSink,
};
use crate::snipper::selection::{Rect, SelectionState};
use image::RgbaImage;
use std::sync::Arc;
#[cfg(target_os = "windows")]
use winit::dpi::PhysicalPosition;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
#[cfg(not(target_os = "windows"))]
use winit::window::Fullscreen;
use winit::window::{Window, WindowLevel};

/// 截图视图创建参数。
pub struct SnipperViewInit {
    /// 全屏底图、窗口候选、配置与宿主端口。
    pub launch: SnipperLaunch,
    /// 媒体任务完成回传。
    pub job_sink: SnipperJobSink,
    /// 录屏状态回传。
    pub recording_sink: RecordingEventSink,
    /// 剪贴板任务完成回传。
    pub clipboard_events: ClipboardEventSink,
    /// 长截图抓屏完成回传。
    pub screen_capture_events: ScreenCaptureEventSink,
}

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
    pub fn new(window_target: &ActiveEventLoop, init: SnipperViewInit) -> Result<Self, UiError> {
        let SnipperViewInit {
            launch,
            job_sink,
            recording_sink,
            clipboard_events,
            screen_capture_events,
        } = init;
        let SnipperLaunch {
            bg_image,
            screen_origin,
            windows,
            runtime,
        } = launch;
        let screen_w = bg_image.width();
        let screen_h = bg_image.height();
        let window = create_overlay_window(window_target, screen_origin, screen_w, screen_h)?;
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
            recording_sink,
            clipboard_events,
            screen_capture_events,
        });
        super::apply_ui_scale(&window, &mut state, screen_w, screen_h);

        window.request_redraw();
        Ok(Self {
            window,
            presenter,
            state,
            bg_image,
        })
    }
}

/// 创建与抓屏内容逐像素对齐的覆盖层窗口。
///
/// Windows 不依赖系统单屏全屏仲裁，而是按虚拟桌面物理原点与抓屏尺寸创建无边框
/// 窗口。这样多屏、负坐标和混合 DPI 下窗口客户区始终与底图一致，不会触发呈现器
/// 的二次缩放；其它平台继续使用窗口系统全屏能力。
fn create_overlay_window(
    window_target: &ActiveEventLoop,
    screen_origin: (i32, i32),
    screen_w: u32,
    screen_h: u32,
) -> Result<Arc<Window>, UiError> {
    let attributes = Window::default_attributes()
        .with_title("Snapin")
        .with_decorations(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_content_protected(true)
        .with_transparent(true);
    let attributes = configure_overlay_bounds(attributes, screen_origin, screen_w, screen_h);
    let window = window_target
        .create_window(attributes)
        .map_err(|error| UiError::Window {
            message: format!("创建全屏截图窗口失败: {error}"),
        })?;
    ensure_window_matches_snapshot(&window, screen_w, screen_h);
    window.set_ime_allowed(true);
    Ok(Arc::new(window))
}

/// 校验覆盖层客户区是否与抓屏像素尺寸完全一致。
///
/// Windows 在缩放切换或窗口创建阶段仍可能调整客户区。此处只做一次主动纠正，
/// 避免呈现器拿到尺寸不一致的表面后把原始底图重采样成模糊图像。
#[cfg(target_os = "windows")]
fn ensure_window_matches_snapshot(window: &Window, screen_w: u32, screen_h: u32) {
    let expected = PhysicalSize::new(screen_w, screen_h);
    let actual = window.inner_size();
    if actual == expected {
        return;
    }

    tracing::warn!(
        "截图覆盖层客户区尺寸与抓屏尺寸不一致，期望: {}x{}，实际: {}x{}，正在请求纠正",
        expected.width,
        expected.height,
        actual.width,
        actual.height
    );
    if let Some(requested) = window.request_inner_size(expected) {
        if requested != expected {
            tracing::warn!(
                "窗口系统未能按抓屏尺寸调整覆盖层，请求后尺寸: {}x{}",
                requested.width,
                requested.height
            );
        }
    }
}

/// 非 Windows 平台由窗口系统全屏仲裁，无需额外纠正。
#[cfg(not(target_os = "windows"))]
fn ensure_window_matches_snapshot(_window: &Window, _screen_w: u32, _screen_h: u32) {}

/// 按平台配置覆盖层的初始物理边界。
#[cfg(target_os = "windows")]
fn configure_overlay_bounds(
    attributes: winit::window::WindowAttributes,
    screen_origin: (i32, i32),
    screen_w: u32,
    screen_h: u32,
) -> winit::window::WindowAttributes {
    attributes
        .with_position(PhysicalPosition::new(screen_origin.0, screen_origin.1))
        .with_inner_size(PhysicalSize::new(screen_w, screen_h))
        .with_resizable(false)
}

/// 按平台配置覆盖层的初始物理边界。
#[cfg(not(target_os = "windows"))]
fn configure_overlay_bounds(
    attributes: winit::window::WindowAttributes,
    _screen_origin: (i32, i32),
    screen_w: u32,
    screen_h: u32,
) -> winit::window::WindowAttributes {
    attributes
        .with_fullscreen(Some(Fullscreen::Borderless(None)))
        .with_inner_size(PhysicalSize::new(screen_w, screen_h))
}

/// 截图交互状态初始化参数。
struct SnipperStateInit {
    runtime: SnipperRuntime,
    windows: Vec<crate::snipper::WindowRect>,
    mode: SnipMode,
    screen_size: (u32, u32),
    job_sink: SnipperJobSink,
    recording_sink: RecordingEventSink,
    clipboard_events: ClipboardEventSink,
    screen_capture_events: ScreenCaptureEventSink,
}

/// 根据启动参数构建截图交互状态。
fn build_initial_state(init: SnipperStateInit) -> SnipperState {
    let SnipperStateInit {
        runtime,
        windows,
        mode,
        screen_size: (screen_w, screen_h),
        job_sink,
        recording_sink,
        clipboard_events,
        screen_capture_events,
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

    state.session.recorder = runtime.recorder;
    state.session.recording_events = recording_sink;
    state.jobs.runtime = Some(Arc::new(SnipperJobRuntime::new(runtime.media, job_sink)));
    state.jobs.clipboard_events = clipboard_events;
    state.jobs.screen_capture_events = screen_capture_events;
    state
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
                bg_image: &mut self.bg_image,
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

    /// 处理录制后台状态。
    pub fn handle_recording_event(&mut self, event: RecordingEvent) {
        let message = match &event {
            RecordingEvent::Warning { message } => Some(message.clone()),
            RecordingEvent::Failed { message } => Some(message.clone()),
            _ => None,
        };
        if event == RecordingEvent::Started {
            self.state.clear_status_message();
        }
        let transition = self
            .state
            .recording
            .as_mut()
            .map(|session| session.handle_event(event));
        match transition {
            Some(RecordingTransition::Finished(_path)) => self.state.clear_status_message(),
            Some(RecordingTransition::Cancelled) => {
                self.state.recording = None;
            }
            Some(RecordingTransition::Restart) => {
                let region = self
                    .state
                    .recording
                    .as_ref()
                    .and_then(|session| session.region());
                if let (Some(session), Some(region)) = (self.state.recording.as_mut(), region) {
                    if let Err(error) = session.start(region) {
                        self.state.set_status_message(error.message);
                    }
                }
            }
            Some(RecordingTransition::Updated) => {}
            None => {}
        }
        if let Some(message) = message {
            self.state
                .set_status_message(snapin_foundation::i18n::tr(&message).to_string());
        }
    }

    /// 处理异步长截图抓屏结果，恢复覆盖层并把画面投喂给拼接器。
    ///
    /// 抓屏期间覆盖层被隐藏以确保捕获到底层应用；无论成功、失败还是结果已过期，
    /// 都必须先恢复窗口，避免因一次抓屏异常让整个截图界面永久不可见。
    pub fn handle_screen_capture_finished(
        &mut self,
        request_id: u64,
        result: Result<RgbaImage, PortError>,
    ) {
        self.window.set_visible(true);
        self.window.focus_window();
        match self
            .state
            .apply_captured_long_frame(&mut self.bg_image, request_id, result)
        {
            CapturedFrameEffect::Stale => return,
            CapturedFrameEffect::NoBgUpdate => {}
            CapturedFrameEffect::BgUpdated => {
                if let Err(error) = self.presenter.replace_background(&self.bg_image, true) {
                    tracing::warn!("更新长截图实时底图失败: {error}");
                }
            }
        }
        self.window.request_redraw();
    }

    /// 取出会话结果；存在结果时会取消仍在执行的后台任务。
    pub fn take_outcome(&mut self) -> Option<SnipperOutcome> {
        let outcome = self.state.session.outcome.take();
        if outcome.is_some() {
            self.state.cancel_pending_job();
            if let Some(session) = self.state.recording.as_mut() {
                session.cancel();
            }
        }
        outcome
    }

    /// 推进录制中的单调时钟秒表；返回显示秒数是否变化。
    pub fn refresh_recording_elapsed(&mut self, now: std::time::Instant) -> bool {
        self.state
            .recording
            .as_mut()
            .is_some_and(|session| session.refresh_recording_elapsed(now))
    }

    /// 当前阶段是否需要周期性 UI 唤醒。
    ///
    /// 录制中用于独立刷新秒表，结果页用于推进内嵌预览；暂停和编码收尾阶段无需
    /// 保持高频唤醒，以降低常驻事件循环负载。
    pub fn needs_periodic_ui_tick(&self) -> bool {
        self.state.recording.as_ref().is_some_and(|session| {
            matches!(
                session.status(),
                super::state::RecordingStatus::Active | super::state::RecordingStatus::Completed
            )
        })
    }

    /// 推进结果页内嵌预览；返回是否需要重绘。
    pub fn advance_recording_preview(&mut self, now: std::time::Instant) -> bool {
        let changed = self
            .state
            .recording
            .as_mut()
            .is_some_and(|session| session.advance_preview(now));
        if changed {
            if let Some(session) = self.state.recording.as_ref() {
                tracing::trace!("录屏预览已推进到第 {} 帧", session.preview_frame_index());
            }
        }
        changed
    }

    /// 返回覆盖层下一次必须主动重绘的时刻。
    pub fn next_redraw_deadline(&self) -> Option<std::time::Instant> {
        let preview = self
            .state
            .recording
            .as_ref()
            .and_then(super::state::RecordingSession::preview_deadline);
        match (self.presenter.present_retry_deadline(), preview) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (left, right) => left.or(right),
        }
    }

    /// 请求重绘。
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// 事件循环退出前取消后台任务并关闭 IME。
    pub fn cancel(&mut self) {
        self.state.cancel_pending_job();
        if let Some(session) = self.state.recording.as_mut() {
            session.cancel();
        }
        self.state.jobs.pending_clipboard = None;
        self.window.set_ime_allowed(false);
    }
}
