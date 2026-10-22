//! 截图会话生命周期管理：封装全屏窗口构建、状态初始化与事件循环驱动。

use super::event_dispatcher::SnipperEventDispatcher;
use super::events::SnipperState;
use super::jobs::{handle_job_outcome, SnipperJobDispatcher, SnipperUserEvent};
use super::presenter::SnipperPresenter;
use super::state::clamp_window_rect;
use super::types::{SnipMode, SnipperOutcome};
use crate::error::UiError;
use crate::snipper::selection::{Rect, SelectionState};
use image::RgbaImage;
use snapin_core::config::Config;
use snapin_platform::WindowRect;
use std::sync::{Arc, Mutex};
use winit::dpi::PhysicalSize;
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::platform::wayland::EventLoopBuilderExtWayland;
use winit::window::{Fullscreen, WindowBuilder};

/// 全屏截图交互会话：纳管物理窗口构建、事件循环与会话生命周期
pub struct SnipperSession {
    bg_image: RgbaImage,
    config: Config,
    mode: SnipMode,
    /// 窗口截图候选矩形（物理像素，底层到顶层）
    windows: Vec<WindowRect>,
}

impl SnipperSession {
    /// 构建新的截图会话实例
    pub fn new(
        bg_image: RgbaImage,
        config: Config,
        mode: SnipMode,
        windows: Vec<WindowRect>,
    ) -> Self {
        Self {
            bg_image,
            config,
            mode,
            windows,
        }
    }

    /// 启动全屏截图窗口并进入交互事件循环，直到用户确认或取消
    pub fn run(self) -> Result<Option<SnipperOutcome>, UiError> {
        let mut builder = EventLoopBuilder::<SnipperUserEvent>::with_user_event();
        builder.with_any_thread(true);
        let event_loop = builder.build().map_err(|e| UiError::Window {
            message: format!("构建截图事件循环失败: {e}"),
        })?;
        let proxy = event_loop.create_proxy();

        let screen_w = self.bg_image.width();
        let screen_h = self.bg_image.height();

        let window = Arc::new(
            WindowBuilder::new()
                .with_title("Snapin")
                .with_decorations(false)
                .with_fullscreen(Some(Fullscreen::Borderless(None)))
                .with_inner_size(PhysicalSize::new(screen_w, screen_h))
                .build(&event_loop)
                .map_err(|e| UiError::Window {
                    message: format!("创建全屏截图窗口失败: {e}"),
                })?,
        );
        window.set_ime_allowed(true);

        let mut presenter = SnipperPresenter::try_new(
            window.clone(),
            screen_w,
            screen_h,
            &self.bg_image,
            self.mode != SnipMode::PickColor,
        )?;

        let config = Arc::new(self.config);
        let mut state = SnipperState::from_config((*config).clone(), self.mode);
        state.window_rects = self
            .windows
            .iter()
            .filter_map(|window| clamp_window_rect(*window, screen_w as f32, screen_h as f32))
            .collect();
        if self.mode == SnipMode::Fullscreen {
            state.selection = SelectionState::Locked {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: screen_w as f32,
                    height: screen_h as f32,
                },
                active_drag: None,
            };
        } else if self.mode == SnipMode::PickColor {
            // 取色器必须显示放大镜；单击复制颜色，Esc 或右键取消。
            state.magnifier.enabled = true;
            state.set_status_message("单击复制颜色，Esc 取消");
        }
        state.job_dispatcher = Some(Arc::new(SnipperJobDispatcher::new(
            proxy,
            Arc::clone(&config),
        )));
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

        if let crate::snipper::selection::SelectionState::Locked { rect, .. } = &state.selection {
            state
                .toolbar
                .update_layout(rect, screen_w as f32, screen_h as f32);
        }

        let final_outcome = Arc::new(Mutex::new(None));
        let outcome_slot = final_outcome.clone();

        window.request_redraw();

        let bg_image = self.bg_image;
        event_loop
            .run(move |event, elwt| {
                elwt.set_control_flow(ControlFlow::Wait);

                match event {
                    Event::WindowEvent { event, .. } => {
                        SnipperEventDispatcher::dispatch(
                            event,
                            &window,
                            &mut presenter,
                            &mut state,
                            &bg_image,
                            &outcome_slot,
                            elwt,
                        );
                    }
                    Event::UserEvent(SnipperUserEvent::JobFinished(outcome)) => {
                        handle_job_outcome(&mut state, outcome);
                        window.request_redraw();
                        SnipperEventDispatcher::check_exit_outcome(&mut state, &outcome_slot, elwt);
                    }
                    Event::LoopExiting => {
                        state.cancel_pending_job();
                        tracing::info!("截图交互窗口事件循环即将退出");
                    }
                    _ => {}
                }
            })
            .map_err(|e| UiError::Window {
                message: format!("运行截图事件循环失败: {e}"),
            })?;

        let outcome = match final_outcome.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        };
        Ok(outcome)
    }
}
