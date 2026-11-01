//! 贴图窗口事件处理与空闲定时清理。

use super::super::window::events::{
    handle_paster_cursor_move, handle_paster_key, handle_paster_mouse_button, handle_paster_scroll,
};
use super::super::window::render::render_paster_to_surface;
use super::super::window::text_session::{refresh_ime_cursor_area, sync_ime_state};
use super::window_builder::PasterInstance;
use super::WorkspaceRuntime;
use std::time::Instant;
use winit::event::{Ime, WindowEvent};
use winit::window::WindowId;

/// 处理一个贴图窗口事件，并在需要时关闭窗口。
pub(crate) fn handle_paster_window_event(
    window_id: WindowId,
    event: WindowEvent,
    runtime: &mut WorkspaceRuntime,
) {
    let should_close = match runtime.instances.get_mut(&window_id) {
        Some(inst) => handle_paster_event(inst, event),
        None => false,
    };
    if !should_close {
        return;
    }

    if let Some(mut removed) = runtime.instances.remove(&window_id) {
        // 关闭前取消后台 OCR/翻译/保存任务，并结束文字会话与 IME，
        // 避免窗口销毁后任务继续占用推理与网络资源。
        removed.state.cancel_pending_job();
        removed.state.cancel_text_session();
        removed.window.set_ime_allowed(false);
        runtime.manager.remove_paster(removed.item_id);
        tracing::info!(
            "贴图已安全关闭: {}，工作区剩余活动贴图: {}",
            removed.item_id,
            runtime.instances.len()
        );
    }
}

/// 分发单个贴图窗口事件；返回是否应关闭该窗口。
fn handle_paster_event(inst: &mut PasterInstance, event: WindowEvent) -> bool {
    match event {
        WindowEvent::CloseRequested => return true,
        WindowEvent::KeyboardInput {
            event: key_event, ..
        } => {
            let text_active = inst.state.has_active_text_session();
            if handle_paster_key(
                &mut inst.state,
                &key_event.logical_key,
                key_event.state,
                &inst.window,
            ) {
                inst.window.request_redraw();
            }
            sync_ime_state(&inst.state, &inst.window, text_active);
        }
        WindowEvent::CursorMoved { position, .. } => {
            let size = inst.window.inner_size();
            if handle_paster_cursor_move(
                &mut inst.state,
                (position.x, position.y),
                (size.width as f32, size.height as f32),
            ) {
                inst.window.request_redraw();
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            if handle_paster_scroll(&mut inst.state, delta, &inst.window) {
                // 缩放会改变文字会话的屏幕位置，同步刷新 IME 候选框锚点
                refresh_ime_cursor_area(&inst.state, &inst.window);
                inst.window.request_redraw();
            }
        }
        WindowEvent::MouseInput { state, button, .. } => {
            let text_active = inst.state.has_active_text_session();
            if handle_paster_mouse_button(&mut inst.state, button, state, &inst.window) {
                inst.window.request_redraw();
            }
            sync_ime_state(&inst.state, &inst.window, text_active);
        }
        WindowEvent::Ime(ime_event) => {
            let need_redraw = match ime_event {
                Ime::Commit(text) => inst.state.apply_ime_commit(&text),
                Ime::Preedit(text, _cursor) => inst.state.apply_ime_preedit(&text),
                _ => false,
            };
            if need_redraw {
                refresh_ime_cursor_area(&inst.state, &inst.window);
                inst.window.request_redraw();
            }
        }
        WindowEvent::ModifiersChanged(modifiers) => {
            inst.state.modifiers = modifiers.state();
        }
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
            let s = scale_factor as f32;
            inst.state.ui_scale = s;
            inst.state.context_menu.set_ui_scale(s);
            inst.window.request_redraw();
        }
        WindowEvent::RedrawRequested => {
            // 过期提示先清除再渲染，确保最后一帧不会闪出已失效的文字
            inst.state.clear_expired_status(Instant::now());
            let size = inst.window.inner_size();
            if let Err(e) = render_paster_to_surface(
                &mut inst.state,
                &mut inst.surface,
                size.width,
                size.height,
            ) {
                tracing::error!("贴图渲染帧缓冲失败: {e}");
            }
        }
        _ => {}
    }
    inst.state.should_close
}

/// 事件循环空闲时清理过期提示、执行宿主周期回调，并返回下一次唤醒时刻。
///
/// 返回 `Some(deadline)` 时调用方应把控制流切换为定时等待，确保提示到期或
/// 托盘/热键轮询到点时即使没有输入事件也能被处理；返回 `None` 时恢复无限等待。
pub(crate) fn handle_workspace_about_to_wait(runtime: &mut WorkspaceRuntime) -> Option<Instant> {
    let now = Instant::now();
    if let Some(on_tick) = runtime.on_tick.as_mut() {
        on_tick();
    }
    let mut earliest: Option<Instant> = runtime
        .on_tick
        .as_ref()
        .map(|_| now + std::time::Duration::from_millis(100));
    for inst in runtime.instances.values_mut() {
        if inst.state.clear_expired_status(now) {
            inst.window.request_redraw();
        }
        if let Some(deadline) = inst.state.status_deadline() {
            earliest = Some(earliest.map_or(deadline, |current| current.min(deadline)));
        }
    }
    earliest
}
