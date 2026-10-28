//! 截图交互会话事件分发器：负责解析平台窗口事件并驱动状态流转与界面呈现。

use super::events::handle_key_input;
use super::events::SnipperState;
use super::jobs::SnipperUserEvent;
use super::mouse::{handle_cursor_move, handle_mouse_button, handle_mouse_wheel};
use super::presenter::SnipperPresenter;
use super::types::SnipperOutcome;
use image::RgbaImage;
use std::sync::{Arc, Mutex};
use winit::event::{Ime, WindowEvent};
use winit::event_loop::EventLoopWindowTarget;
use winit::window::Window;

/// 截图窗口事件分发器
pub struct SnipperEventDispatcher;

impl SnipperEventDispatcher {
    /// 分发单一窗口事件并根据需要驱动界面重绘或安全退出
    pub fn dispatch(
        event: WindowEvent,
        window: &Window,
        presenter: &mut SnipperPresenter,
        state: &mut SnipperState,
        bg_image: &RgbaImage,
        outcome_slot: &Arc<Mutex<Option<SnipperOutcome>>>,
        elwt: &EventLoopWindowTarget<SnipperUserEvent>,
    ) {
        let screen_w = bg_image.width();
        let screen_h = bg_image.height();

        match event {
            WindowEvent::RedrawRequested => {
                if let Err(e) = presenter.render_and_present(state, bg_image) {
                    tracing::error!("向屏幕刷新帧缓冲失败: {e}");
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let s = scale_factor as f32;
                state.toolbar.ui_scale = s;
                state.magnifier.ui_scale = s;
                window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let need_redraw = handle_cursor_move(
                    state,
                    (position.x, position.y),
                    screen_w as f32,
                    screen_h as f32,
                );
                if need_redraw {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if handle_mouse_wheel(state, delta) {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: btn_state,
                button,
                ..
            } => {
                let need_redraw = handle_mouse_button(state, button, btn_state, bg_image);
                if need_redraw {
                    window.request_redraw();
                }
                Self::check_exit_outcome(state, outcome_slot, elwt);
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                let need_redraw =
                    handle_key_input(state, &key_event.logical_key, key_event.state, bg_image);
                if need_redraw {
                    window.request_redraw();
                }
                Self::check_exit_outcome(state, outcome_slot, elwt);
            }
            WindowEvent::Ime(ime_event) => {
                let need_redraw = match ime_event {
                    Ime::Commit(text) => {
                        if let Some(session) = &mut state.active_text_input {
                            session.push_commit(&text);
                            true
                        } else {
                            false
                        }
                    }
                    Ime::Preedit(text, _cursor) => {
                        if let Some(session) = &mut state.active_text_input {
                            session.set_preedit(&text);
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if need_redraw {
                    if let Some(session) = &state.active_text_input {
                        window.set_ime_cursor_area(
                            winit::dpi::PhysicalPosition::new(
                                session.pos.0 as i32,
                                session.pos.1 as i32,
                            ),
                            winit::dpi::PhysicalSize::new(24, session.font_size as u32),
                        );
                    }
                    window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                state.modifiers = mods.state();
            }
            WindowEvent::CloseRequested => {
                state.cancel_pending_job();
                if let Ok(mut g) = outcome_slot.lock() {
                    *g = Some(SnipperOutcome::Cancelled);
                }
                elwt.exit();
            }
            _ => {}
        }
    }

    /// 检查是否存在有效结果并安全退出事件循环
    pub(crate) fn check_exit_outcome(
        state: &mut SnipperState,
        outcome_slot: &Arc<Mutex<Option<SnipperOutcome>>>,
        elwt: &EventLoopWindowTarget<SnipperUserEvent>,
    ) {
        if state.outcome.is_some() {
            state.cancel_pending_job();
            if let Ok(mut g) = outcome_slot.lock() {
                *g = state.outcome.take();
            }
            elwt.exit();
        }
    }
}
