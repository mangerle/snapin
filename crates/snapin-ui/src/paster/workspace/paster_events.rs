//! 贴图窗口事件处理与空闲定时清理。

use super::super::window::events::{
    handle_paster_cursor_move, handle_paster_key, handle_paster_mouse_button, handle_paster_scroll,
};
use super::super::window::render::render_paster_to_surface;
use super::super::window::text_session::{refresh_ime_cursor_area, sync_ime_state};
use super::window_builder::PasterInstance;
use super::WorkspaceRuntime;
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
    if let Err(error) =
        render_paster_to_surface(&mut inst.state, &mut inst.surface, size.width, size.height)
    {
        tracing::error!("贴图渲染帧缓冲失败: {error}");
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
