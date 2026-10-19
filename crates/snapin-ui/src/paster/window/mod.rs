//! 贴图浮动置顶窗口渲染主循环。

pub mod events;
pub mod render;

pub use events::PasterWindowState;
pub use render::render_paster_to_surface;

use crate::paster::manager::PasterItem;
use events::{
    handle_paster_cursor_move, handle_paster_key, handle_paster_mouse_button, handle_paster_scroll,
};
use snapin_core::error::SnapinError;
use snapin_platform::pin_window_above;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::platform::wayland::{EventLoopBuilderExtWayland, WindowBuilderExtWayland};
use winit::window::{Window, WindowBuilder};

/// 获取贴图子系统全局共享的后台异步运行时（单工作线程，避免每个贴图窗口重复派生线程和运行时）
fn shared_paster_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("初始化贴图子系统共享异步运行时失败")
    })
}

/// 贴图窗口自定义用户事件
#[derive(Debug, Clone, Copy)]
pub enum PasterUserEvent {
    ToggleVisibility,
}

/// 启动独立贴图浮动置顶窗口
pub fn spawn_paster_window(item: PasterItem) -> Result<(), SnapinError> {
    let mut builder = EventLoopBuilder::<PasterUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = builder.build().map_err(|e| SnapinError::WindowError {
        message: format!("构建贴图事件循环失败: {e}"),
    })?;
    let event_proxy = event_loop.create_proxy();
    let title = item.title.clone();
    let initial_w = item.pixmap.width();
    let initial_h = item.pixmap.height();
    let (init_x, init_y) = item.position;

    let window = Arc::new(
        WindowBuilder::new()
            .with_name("snapin-pin", "snapin-pin")
            .with_title(&title)
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size(PhysicalSize::new(initial_w, initial_h))
            .with_position(PhysicalPosition::new(init_x, init_y))
            .build(&event_loop)
            .map_err(|e| SnapinError::WindowError {
                message: format!("创建贴图置顶窗口失败: {e}"),
            })?,
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

    let cfg = snapin_core::config::load_or_create_config(None).unwrap_or_default();
    let enable_above = cfg.paster.enable_above_extension;

    // 触发配套 GNOME 扩展将本窗口强制置顶 (受配置开关控制)
    if enable_above {
        let title_clone = title.clone();
        shared_paster_runtime().spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            if let Err(e) = pin_window_above(&title_clone).await {
                tracing::warn!("调用 GNOME 扩展置顶窗口失败: {e}");
            }
        });
    }

    let raw_scale = window.scale_factor() as f32;
    let auto_scale = if raw_scale > 1.05 { raw_scale } else { 1.0 };
    let mut state = PasterWindowState::from_config(item, cfg);
    state.ui_scale = auto_scale;
    state.context_menu.set_ui_scale(auto_scale);

    // 监听 SIGUSR1 信号，收到信号后批量切换窗口显示/隐藏状态
    shared_paster_runtime().spawn(async move {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1())
        {
            while sig.recv().await.is_some() {
                let _ = event_proxy.send_event(PasterUserEvent::ToggleVisibility);
            }
        }
    });

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::UserEvent(PasterUserEvent::ToggleVisibility) => {
                    state.item.is_visible = !state.item.is_visible;
                    window.set_visible(state.item.is_visible);
                }
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
        })
        .map_err(|e| SnapinError::WindowError {
            message: format!("运行贴图事件循环失败: {e}"),
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
    elwt: &winit::event_loop::EventLoopWindowTarget<PasterUserEvent>,
) {
    match event {
        WindowEvent::RedrawRequested => {
            let size = ctx.window.inner_size();
            if let Err(e) = render_paster_to_surface(state, ctx.surface, size.width, size.height) {
                tracing::error!("贴图渲染失败: {e}");
            }
        }
        WindowEvent::CursorMoved { position, .. } => {
            if handle_paster_cursor_move(state, (position.x, position.y)) {
                ctx.window.request_redraw();
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
