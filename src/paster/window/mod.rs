//! 贴图浮动置顶窗口渲染主循环。

pub mod events;
pub mod render;

pub use events::PasterWindowState;
pub use render::render_paster_to_surface;

use crate::error::SnapinError;
use crate::paster::manager::PasterItem;
use crate::platform::pin_window_above;
use events::{handle_paster_key, handle_paster_mouse_button, handle_paster_scroll};
use std::sync::Arc;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::platform::wayland::EventLoopBuilderExtWayland;
use winit::window::{Window, WindowBuilder};

/// 启动独立贴图浮动置顶窗口
pub fn spawn_paster_window(item: PasterItem) -> Result<(), SnapinError> {
    let mut builder = EventLoopBuilder::new();
    builder.with_any_thread(true);
    let event_loop = builder.build()?;
    let title = item.title.clone();
    let initial_w = item.pixmap.width();
    let initial_h = item.pixmap.height();
    let (init_x, init_y) = item.position;

    let window = Arc::new(
        WindowBuilder::new()
            .with_title(&title)
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size(PhysicalSize::new(initial_w, initial_h))
            .with_position(PhysicalPosition::new(init_x, init_y))
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

    // 触发配套 GNOME 扩展将本窗口强制置顶
    let title_clone = title.clone();
    std::thread::spawn(move || {
        // 短暂延迟 80ms 等待 Mutter 建立窗口映射
        std::thread::sleep(std::time::Duration::from_millis(80));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        if let Ok(rt) = rt {
            rt.block_on(async {
                if let Err(e) = pin_window_above(&title_clone).await {
                    tracing::warn!("调用 GNOME 扩展置顶窗口失败: {e}");
                }
            });
        }
    });

    let mut state = PasterWindowState::new(item);

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);

        match event {
            Event::WindowEvent { event, .. } => {
                let mut ctx = PasterWindowContext {
                    window: &window,
                    surface: &mut surface,
                };
                dispatch_paster_event(event, &mut ctx, &mut state, elwt);
            }
            Event::LoopExiting => {
                tracing::info!("贴图窗口退出: {title}");
            }
            _ => {}
        }
    })?;

    Ok(())
}

/// 贴图窗口上下文数据
struct PasterWindowContext<'a> {
    window: &'a Arc<Window>,
    surface: &'a mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
}

/// 分发贴图窗口输入事件与重绘
fn dispatch_paster_event(
    event: WindowEvent,
    ctx: &mut PasterWindowContext<'_>,
    state: &mut PasterWindowState,
    elwt: &winit::event_loop::EventLoopWindowTarget<()>,
) {
    match event {
        WindowEvent::RedrawRequested => {
            let size = ctx.window.inner_size();
            if let Err(e) =
                render_paster_to_surface(&state.item, ctx.surface, size.width, size.height)
            {
                tracing::error!("贴图渲染失败: {e}");
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            if handle_paster_scroll(state, delta, ctx.window) {
                ctx.window.request_redraw();
            }
        }
        WindowEvent::MouseInput {
            state: btn_state,
            button,
            ..
        } => {
            if handle_paster_mouse_button(state, button, btn_state, ctx.window) {
                ctx.window.request_redraw();
            }
            if state.should_close {
                elwt.exit();
            }
        }
        WindowEvent::KeyboardInput {
            event: key_event, ..
        } => {
            if handle_paster_key(state, &key_event.logical_key, key_event.state, ctx.window) {
                ctx.window.request_redraw();
            }
            if state.should_close {
                elwt.exit();
            }
        }
        WindowEvent::CloseRequested => {
            elwt.exit();
        }
        _ => {}
    }
}
