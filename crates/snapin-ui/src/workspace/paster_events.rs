//! 贴图窗口事件处理与空闲定时清理。

use super::window_builder::PasterInstance;
use super::WorkspaceRuntime;
use crate::paster::window::events::{
    handle_paster_cursor_move, handle_paster_key, handle_paster_mouse_button, handle_paster_scroll,
};
use crate::paster::window::render::render_paster_to_surface;
use crate::paster::window::text_session::{refresh_ime_cursor_area, sync_ime_state};
use crate::paster::PresentOutcome;
use std::time::Instant;
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
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
    if should_close {
        remove_paster_instance(runtime, window_id);
    }
}

/// 安全移除一个贴图窗口实例并释放其后台任务与输入法状态。
pub(crate) fn remove_paster_instance(runtime: &mut WorkspaceRuntime, window_id: WindowId) {
    if let Some(mut removed) = runtime.instances.remove(&window_id) {
        // 关闭前取消后台 OCR/翻译/保存任务，并结束文字会话与 IME，
        // 避免窗口销毁后任务继续占用推理与网络资源。
        removed.state.cancel_pending_job();
        removed.state.cancel_text_session();
        removed.state.cancel_pending_clipboard();
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
        WindowEvent::CloseRequested => true,
        WindowEvent::KeyboardInput {
            event: key_event, ..
        } => handle_keyboard_event(inst, key_event),
        WindowEvent::CursorMoved { position, .. } => {
            handle_cursor_event(inst, (position.x, position.y))
        }
        WindowEvent::MouseWheel { delta, .. } => handle_wheel_event(inst, delta),
        WindowEvent::MouseInput { state, button, .. } => {
            handle_mouse_input_event(inst, button, state)
        }
        WindowEvent::Ime(ime_event) => handle_ime_event(inst, ime_event),
        WindowEvent::ModifiersChanged(modifiers) => {
            inst.state.modifiers = modifiers.state();
            inst.state.should_close
        }
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
            handle_scale_factor_event(inst, scale_factor)
        }
        WindowEvent::RedrawRequested => handle_redraw_event(inst),
        _ => false,
    }
}

/// 处理键盘输入并同步 IME 状态。
fn handle_keyboard_event(inst: &mut PasterInstance, event: KeyEvent) -> bool {
    let text_active = inst.state.has_active_text_session();
    if handle_paster_key(
        &mut inst.state,
        &event.logical_key,
        event.state,
        &inst.window,
    ) {
        inst.window.request_redraw();
    }
    sync_ime_state(&inst.state, &inst.window, text_active);
    inst.state.should_close
}

/// 处理光标移动并按需重绘。
fn handle_cursor_event(inst: &mut PasterInstance, position: (f64, f64)) -> bool {
    let size = inst.window.inner_size();
    if handle_paster_cursor_move(
        &mut inst.state,
        position,
        (size.width as f32, size.height as f32),
    ) {
        inst.window.request_redraw();
    }
    inst.state.should_close
}

/// 处理滚轮缩放并刷新 IME 候选框锚点。
fn handle_wheel_event(inst: &mut PasterInstance, delta: MouseScrollDelta) -> bool {
    if handle_paster_scroll(&mut inst.state, delta, &inst.window) {
        refresh_ime_cursor_area(&inst.state, &inst.window);
        inst.window.request_redraw();
    }
    inst.state.should_close
}

/// 处理鼠标按键并同步 IME 状态。
fn handle_mouse_input_event(
    inst: &mut PasterInstance,
    button: MouseButton,
    state: ElementState,
) -> bool {
    let text_active = inst.state.has_active_text_session();
    if handle_paster_mouse_button(&mut inst.state, button, state, &inst.window) {
        inst.window.request_redraw();
    }
    sync_ime_state(&inst.state, &inst.window, text_active);
    inst.state.should_close
}

/// 处理 IME 提交与预编辑文本。
fn handle_ime_event(inst: &mut PasterInstance, event: Ime) -> bool {
    let need_redraw = match event {
        Ime::Commit(text) => inst.state.apply_ime_commit(&text),
        Ime::Preedit(text, _cursor) => inst.state.apply_ime_preedit(&text),
        _ => false,
    };
    if need_redraw {
        refresh_ime_cursor_area(&inst.state, &inst.window);
        inst.window.request_redraw();
    }
    inst.state.should_close
}

/// 处理显示器缩放变化。
fn handle_scale_factor_event(inst: &mut PasterInstance, scale_factor: f64) -> bool {
    let scale = scale_factor as f32;
    inst.state.viewport.ui_scale = scale;
    inst.state.interaction.context_menu.set_ui_scale(scale);
    inst.window.request_redraw();
    inst.state.should_close
}

/// 清理过期提示并渲染贴图帧。
fn handle_redraw_event(inst: &mut PasterInstance) -> bool {
    inst.state.clear_expired_status(Instant::now());
    let size = inst.window.inner_size();
    match render_paster_to_surface(&mut inst.state, &mut inst.surface, size.width, size.height) {
        Ok(PresentOutcome::Presented) => {
            inst.render_error_reported = false;
        }
        Ok(PresentOutcome::Deferred) => {
            tracing::debug!("合成器尚未归还贴图缓冲区，本帧已安全延期");
        }
        Err(error) => {
            if !inst.render_error_reported {
                tracing::error!("贴图渲染帧缓冲失败: {error}");
                inst.render_error_reported = true;
            }
        }
    }
    inst.state.should_close
}

/// 事件循环空闲时清理过期提示与呈现重试，并返回下一次唤醒时刻。
///
/// 返回 `Some(deadline)` 时调用方应把控制流切换为定时等待，确保状态提示到期或
/// 表面重试到点时即使没有输入事件也能被处理；返回 `None` 时恢复无限等待。
/// 宿主动作由 [`PasterAppEvent::HostWake`](crate::workspace::PasterAppEvent::HostWake)
/// 推送唤醒，不再依赖固定周期轮询。
pub(crate) fn handle_workspace_about_to_wait(runtime: &mut WorkspaceRuntime) -> Option<Instant> {
    let now = Instant::now();
    let mut earliest: Option<Instant> = None;
    for inst in runtime.instances.values_mut() {
        if inst.state.clear_expired_status(now) {
            inst.window.request_redraw();
        }
        if let Some(deadline) = inst.state.status_deadline() {
            earliest = Some(earliest.map_or(deadline, |current| current.min(deadline)));
        }
        if let Some(deadline) = inst.surface.present_retry_deadline() {
            if deadline <= now {
                inst.window.request_redraw();
            } else {
                earliest = Some(earliest.map_or(deadline, |current| current.min(deadline)));
            }
        }
    }
    earliest
}
