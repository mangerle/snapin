//! 截图交互会话全屏窗口与双缓冲渲染事件循环。

pub mod actions;
pub mod annotation_builder;
pub mod drag_calculator;
pub mod events;
pub mod mouse;
pub mod text_input;
pub mod types;

pub use events::SnipperState;
pub use types::SnipperOutcome;

use crate::error::UiError;
use crate::snipper::canvas::SnipperCanvas;
use crate::surface::blit_pixmap_to_surface;
use events::handle_key_input;
use image::RgbaImage;
use mouse::{handle_cursor_move, handle_mouse_button, handle_mouse_wheel};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use tiny_skia::Pixmap;
pub use types::SnipMode;
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::platform::wayland::EventLoopBuilderExtWayland;
use winit::window::{Fullscreen, Window, WindowBuilder};

/// 启动全屏截图交互会话，进入事件循环直至用户确认或取消
pub fn run_snipper_session(
    bg_image: RgbaImage,
    cfg: snapin_core::config::Config,
    mode: SnipMode,
) -> Result<Option<SnipperOutcome>, UiError> {
    let mut builder = EventLoopBuilder::new();
    builder.with_any_thread(true);
    let event_loop = builder.build().map_err(|e| UiError::Window {
        message: format!("构建截图事件循环失败: {e}"),
    })?;
    let screen_w = bg_image.width();
    let screen_h = bg_image.height();

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

    let context = softbuffer::Context::new(window.clone()).map_err(|e| UiError::Softbuffer {
        message: e.to_string(),
    })?;
    let mut surface =
        softbuffer::Surface::new(&context, window.clone()).map_err(|e| UiError::Softbuffer {
            message: e.to_string(),
        })?;

    let w = NonZeroU32::new(screen_w).ok_or_else(|| UiError::Softbuffer {
        message: "无效的屏幕宽度".to_string(),
    })?;
    let h = NonZeroU32::new(screen_h).ok_or_else(|| UiError::Softbuffer {
        message: "无效的屏幕高度".to_string(),
    })?;
    surface.resize(w, h).map_err(|e| UiError::Softbuffer {
        message: format!("初始化帧缓冲尺寸失败: {e}"),
    })?;

    let (bg_pixmap, dimmed_bg) = create_pixmaps_fast(&bg_image)?;

    // 在进入事件循环前，立即将暗化蒙层写入 softbuffer 共享表面，确保窗口映射瞬间即为暗色
    let _ = blit_pixmap_to_surface(&mut surface, &dimmed_bg);

    let mut render_pixmap =
        Pixmap::new(screen_w, screen_h).ok_or_else(|| UiError::PixmapAlloc {
            width: screen_w,
            height: screen_h,
        })?;

    let mut state = SnipperState::from_config(cfg, mode);
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

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::WindowEvent { event, .. } => {
                    let mut ctx = WindowContext {
                        window: &window,
                        surface: &mut surface,
                        bg_pixmap: &bg_pixmap,
                        dimmed_bg: &dimmed_bg,
                        bg_image: &bg_image,
                        render_pixmap: &mut render_pixmap,
                        outcome_slot: &outcome_slot,
                    };
                    dispatch_window_event(event, &mut ctx, &mut state, elwt);
                }
                Event::LoopExiting => {
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

/// 窗口渲染与上下文数据集合
struct WindowContext<'a> {
    window: &'a Arc<Window>,
    surface: &'a mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    bg_pixmap: &'a Pixmap,
    dimmed_bg: &'a Pixmap,
    bg_image: &'a RgbaImage,
    render_pixmap: &'a mut Pixmap,
    outcome_slot: &'a Arc<Mutex<Option<SnipperOutcome>>>,
}

/// 分发窗口内部事件并执行光栅化渲染
fn dispatch_window_event(
    event: WindowEvent,
    ctx: &mut WindowContext<'_>,
    state: &mut SnipperState,
    elwt: &winit::event_loop::EventLoopWindowTarget<()>,
) {
    let screen_w = ctx.bg_image.width();
    let screen_h = ctx.bg_image.height();

    match event {
        WindowEvent::RedrawRequested => {
            let pending_stroke = state.toolbar.active_tool.as_ref().and_then(|tool| {
                if !state.pending_stroke_points.is_empty() {
                    Some((
                        tool,
                        state.pending_stroke_points.as_slice(),
                        state.next_badge_number,
                    ))
                } else {
                    None
                }
            });

            let canvas = SnipperCanvas {
                original_bg: ctx.bg_pixmap,
                dimmed_bg: ctx.dimmed_bg,
                original_rgba: ctx.bg_image,
                selection: &state.selection,
                cursor_pos: state.cursor_pos,
                magnifier: &state.magnifier,
                toolbar: Some(&state.toolbar),
                annotations: Some(&state.annotations),
                ui_scale: state.toolbar.ui_scale,
                accent_color: state.config.interface.accent_color.to_rgb(),
                pending_stroke,
                active_text_input: state.active_text_input.as_ref(),
            };

            let mut pixmap_mut = ctx.render_pixmap.as_mut();
            canvas.render(&mut pixmap_mut);

            if let Err(e) = blit_pixmap_to_surface(ctx.surface, ctx.render_pixmap) {
                tracing::error!("向屏幕刷新帧缓冲失败: {e}");
            }
        }
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
            let s = scale_factor as f32;
            state.toolbar.ui_scale = s;
            state.magnifier.ui_scale = s;
            ctx.window.request_redraw();
        }
        WindowEvent::CursorMoved { position, .. } => {
            let need_redraw = handle_cursor_move(
                state,
                (position.x, position.y),
                ctx.window,
                screen_w as f32,
                screen_h as f32,
            );
            if need_redraw {
                ctx.window.request_redraw();
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            if handle_mouse_wheel(state, delta) {
                ctx.window.request_redraw();
            }
        }
        WindowEvent::MouseInput {
            state: btn_state,
            button,
            ..
        } => {
            let need_redraw = handle_mouse_button(state, button, btn_state, ctx.bg_image);
            if need_redraw {
                ctx.window.request_redraw();
            }
            check_exit_outcome(state, ctx.outcome_slot, elwt);
        }
        WindowEvent::KeyboardInput {
            event: key_event, ..
        } => {
            let need_redraw =
                handle_key_input(state, &key_event.logical_key, key_event.state, ctx.bg_image);
            if need_redraw {
                ctx.window.request_redraw();
            }
            check_exit_outcome(state, ctx.outcome_slot, elwt);
        }
        WindowEvent::ModifiersChanged(mods) => {
            state.modifiers = mods.state();
        }
        WindowEvent::CloseRequested => {
            if let Ok(mut g) = ctx.outcome_slot.lock() {
                *g = Some(SnipperOutcome::Cancelled);
            }
            elwt.exit();
        }
        _ => {}
    }
}

/// 检查是否存在有效结果并安全退出事件循环
fn check_exit_outcome(
    state: &mut SnipperState,
    outcome_slot: &Arc<Mutex<Option<SnipperOutcome>>>,
    elwt: &winit::event_loop::EventLoopWindowTarget<()>,
) {
    if state.outcome.is_some() {
        if let Ok(mut g) = outcome_slot.lock() {
            *g = state.outcome.take();
        }
        elwt.exit();
    }
}

/// 单遍高速生成原图底图与暗色蒙层底图 (单次内存遍历，CPU 缓存高度命中)
fn create_pixmaps_fast(img: &RgbaImage) -> Result<(Pixmap, Pixmap), UiError> {
    let w = img.width();
    let h = img.height();
    let mut bg = Pixmap::new(w, h).ok_or_else(|| UiError::PixmapAlloc {
        width: w,
        height: h,
    })?;
    let mut dimmed = Pixmap::new(w, h).ok_or_else(|| UiError::PixmapAlloc {
        width: w,
        height: h,
    })?;

    let src = img.as_raw();
    let dst_bg = bg.data_mut();
    let dst_dim = dimmed.data_mut();

    // 内存单遍流转：同时填充原图与定点整数暗化蒙层 ((val * 107) >> 8)
    for ((d_bg, d_dim), s) in dst_bg
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(dst_dim.as_chunks_mut::<4>().0.iter_mut())
        .zip(src.as_chunks::<4>().0)
    {
        d_bg.copy_from_slice(s);
        d_dim[0] = ((s[0] as u16 * 107) >> 8) as u8;
        d_dim[1] = ((s[1] as u16 * 107) >> 8) as u8;
        d_dim[2] = ((s[2] as u16 * 107) >> 8) as u8;
        d_dim[3] = s[3];
    }

    Ok((bg, dimmed))
}
