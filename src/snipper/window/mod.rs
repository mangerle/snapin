//! 截图交互会话全屏窗口与双缓冲渲染事件循环。

pub mod actions;
pub mod events;
pub mod types;

pub use events::SnipperState;
pub use types::SnipperOutcome;

use crate::error::SnapinError;
use crate::snipper::canvas::SnipperCanvas;
use events::{handle_cursor_move, handle_key_input, handle_mouse_button, handle_mouse_wheel};
use image::RgbaImage;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use tiny_skia::Pixmap;
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::platform::wayland::EventLoopBuilderExtWayland;
use winit::window::{Fullscreen, Window, WindowBuilder};

/// 启动全屏截图交互会话，进入事件循环直至用户确认或取消
pub fn run_snipper_session(bg_image: RgbaImage) -> Result<Option<SnipperOutcome>, SnapinError> {
    let mut builder = EventLoopBuilder::new();
    builder.with_any_thread(true);
    let event_loop = builder.build()?;
    let screen_w = bg_image.width();
    let screen_h = bg_image.height();

    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Snapin")
            .with_decorations(false)
            .with_fullscreen(Some(Fullscreen::Borderless(None)))
            .with_inner_size(PhysicalSize::new(screen_w, screen_h))
            .build(&event_loop)?,
    );

    let context =
        softbuffer::Context::new(window.clone()).map_err(|e| SnapinError::SoftbufferError {
            message: e.to_string(),
        })?;
    let mut surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| {
        SnapinError::SoftbufferError {
            message: e.to_string(),
        }
    })?;

    let w = NonZeroU32::new(screen_w).ok_or_else(|| SnapinError::SoftbufferError {
        message: "无效的屏幕宽度".to_string(),
    })?;
    let h = NonZeroU32::new(screen_h).ok_or_else(|| SnapinError::SoftbufferError {
        message: "无效的屏幕高度".to_string(),
    })?;
    surface
        .resize(w, h)
        .map_err(|e| SnapinError::SoftbufferError {
            message: format!("初始化帧缓冲尺寸失败: {e}"),
        })?;

    let bg_pixmap = rgba_to_pixmap(&bg_image)?;
    let mut dimmed_bg = bg_pixmap.clone();
    if let Some(screen_rect) =
        tiny_skia::Rect::from_xywh(0.0, 0.0, screen_w as f32, screen_h as f32)
    {
        let mut mask_paint = tiny_skia::Paint::default();
        mask_paint.set_color_rgba8(0, 0, 0, 148);
        dimmed_bg.fill_rect(
            screen_rect,
            &mask_paint,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let mut render_pixmap =
        Pixmap::new(screen_w, screen_h).ok_or_else(|| SnapinError::SoftbufferError {
            message: "分配渲染双缓冲区失败".to_string(),
        })?;

    let mut state = SnipperState::new();
    let raw_scale = window.scale_factor() as f32;
    let auto_scale = if raw_scale > 1.05 {
        raw_scale
    } else if screen_w >= 2560 {
        2.0
    } else {
        1.0
    };
    state.toolbar.ui_scale = auto_scale;
    state.magnifier.ui_scale = auto_scale;

    let final_outcome = Arc::new(Mutex::new(None));
    let outcome_slot = final_outcome.clone();

    event_loop.run(move |event, elwt| {
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
                pending_stroke,
            };

            let mut pixmap_mut = ctx.render_pixmap.as_mut();
            canvas.render(&mut pixmap_mut);

            if let Err(e) = blit_to_surface(ctx.surface, ctx.render_pixmap) {
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

/// 将 RgbaImage 转换为 tiny_skia Pixmap
fn rgba_to_pixmap(img: &RgbaImage) -> Result<Pixmap, SnapinError> {
    let mut pixmap =
        Pixmap::new(img.width(), img.height()).ok_or_else(|| SnapinError::SoftbufferError {
            message: "分配背景底图 Pixmap 失败".to_string(),
        })?;
    pixmap.data_mut().copy_from_slice(img.as_raw());
    Ok(pixmap)
}

/// 将 tiny-skia 像素高速复制到 softbuffer 共享内存缓冲区并呈现
fn blit_to_surface(
    surface: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    pixmap: &Pixmap,
) -> Result<(), SnapinError> {
    let mut buffer = surface
        .buffer_mut()
        .map_err(|e| SnapinError::SoftbufferError {
            message: format!("获取绘图表面缓冲区失败: {e}"),
        })?;

    let src_bytes = pixmap.data();
    for (dest, src) in buffer.iter_mut().zip(src_bytes.as_chunks::<4>().0) {
        *dest = ((src[0] as u32) << 16) | ((src[1] as u32) << 8) | (src[2] as u32);
    }

    buffer.present().map_err(|e| SnapinError::SoftbufferError {
        message: format!("帧缓冲提交显示失败: {e}"),
    })?;

    Ok(())
}
