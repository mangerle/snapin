//! 截图交互会话事件分发器：负责解析平台窗口事件并驱动状态流转与界面呈现。

use super::cursor::refresh_cursor;
use super::events::handle_key_input;
use super::events::SnipperState;
use super::mouse::{
    handle_cursor_move, handle_mouse_button, handle_mouse_wheel, resolve_long_capture_wheel_delta,
};
use super::presenter::SnipperPresenter;
use super::types::SnipperOutcome;
use crate::ports::LongCaptureFrameRequest;
use crate::snipper::selection::SelectionState;
use image::RgbaImage;
use snapin_foundation::i18n::tr;
use std::time::Instant;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{Ime, WindowEvent};
use winit::window::Window;

/// 截图窗口事件分发器
pub struct SnipperEventDispatcher;

/// 单次窗口事件派发所需的运行时引用。
pub(crate) struct SnipperDispatchContext<'a> {
    pub window: &'a Window,
    pub presenter: &'a mut SnipperPresenter,
    pub state: &'a mut SnipperState,
    pub bg_image: &'a mut RgbaImage,
}

impl SnipperEventDispatcher {
    /// 分发单一窗口事件并根据需要驱动界面重绘或安全退出
    pub(crate) fn dispatch(event: WindowEvent, mut context: SnipperDispatchContext<'_>) {
        match event {
            WindowEvent::RedrawRequested => render_frame(&mut context),
            WindowEvent::Resized(size) => handle_resize(&mut context, size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                apply_scale_factor(&mut context, scale_factor as f32);
            }
            WindowEvent::CursorMoved { position, .. } => {
                handle_cursor_position(&mut context, position);
            }
            WindowEvent::MouseWheel { delta, .. } => handle_wheel(&mut context, delta),
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => handle_pointer_button(&mut context, button, button_state),
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => handle_keyboard(&mut context, &key_event),
            WindowEvent::Ime(ime_event) => handle_ime(&mut context, ime_event),
            WindowEvent::ModifiersChanged(modifiers) => {
                context.state.viewport.modifiers = modifiers.state();
            }
            WindowEvent::CloseRequested => {
                context.state.abort_long_capture();
                context.state.cancel_pending_job();
                let completed_path = context
                    .state
                    .recording
                    .as_ref()
                    .and_then(|session| session.artifact())
                    .map(|artifact| artifact.path.clone());
                context.state.session.outcome = Some(
                    completed_path
                        .map_or(SnipperOutcome::Cancelled, SnipperOutcome::RecordingSaved),
                );
            }
            _ => {}
        }

        // 任意事件都可能改变选区、工具或光标位置，统一在此刷新系统光标形状
        refresh_cursor(context.window, context.state);
    }
}

/// 处理窗口尺寸变化，重建呈现器缓冲并安排下一次重绘。
fn handle_resize(context: &mut SnipperDispatchContext<'_>, size: PhysicalSize<u32>) {
    if size.width == 0 || size.height == 0 {
        return;
    }
    if let Err(error) = context
        .presenter
        .resize(size.width, size.height, context.bg_image)
    {
        tracing::warn!(
            "重建截图窗口帧缓冲失败，尺寸: {}x{}，原因: {error}",
            size.width,
            size.height
        );
        return;
    }
    super::apply_ui_scale(context.window, context.state, size.width, size.height);
    context.window.request_redraw();
}

/// 执行一帧渲染并记录呈现失败。
fn render_frame(context: &mut SnipperDispatchContext<'_>) {
    if let Err(error) = context
        .presenter
        .render_and_present(context.state, context.bg_image)
    {
        tracing::error!("向屏幕刷新帧缓冲失败: {error}");
    }
}

/// 同步 DPI 缩放并请求重绘。
fn apply_scale_factor(context: &mut SnipperDispatchContext<'_>, _scale_factor: f32) {
    let size = context.window.inner_size();
    super::apply_ui_scale(context.window, context.state, size.width, size.height);
    context.window.request_redraw();
}

/// 将物理光标位置交给选区与标注交互状态机。
fn handle_cursor_position(
    context: &mut SnipperDispatchContext<'_>,
    position: PhysicalPosition<f64>,
) {
    let screen_width = context.bg_image.width() as f32;
    let screen_height = context.bg_image.height() as f32;
    let need_redraw = handle_cursor_move(
        context.state,
        (position.x, position.y),
        screen_width,
        screen_height,
    );
    if need_redraw {
        context.window.request_redraw();
    }
}

/// 处理鼠标滚轮并请求重绘。
fn handle_wheel(context: &mut SnipperDispatchContext<'_>, delta: winit::event::MouseScrollDelta) {
    let capturing_long = context
        .state
        .long_capture
        .as_ref()
        .is_some_and(|session| session.is_capturing && !session.is_editing);
    if !handle_mouse_wheel(context.state, delta) {
        return;
    }
    if capturing_long {
        let mut wheel_overflowed = false;
        if let Some(session) = context.state.long_capture.as_mut() {
            if let Some(wheel_delta) = resolve_long_capture_wheel_delta(session.direction, delta) {
                session.enqueue_wheel_delta(wheel_delta);
                wheel_overflowed = session.take_wheel_overflowed();
            }
        }
        let dispatched = dispatch_pending_long_capture_frame(
            context.window,
            context.state,
            context.bg_image.dimensions(),
        );
        if wheel_overflowed {
            context
                .state
                .set_status_message(tr("滚动过快，已限流等待拼接对齐"));
            if !dispatched {
                context.window.request_redraw();
            }
        }
    } else {
        context.window.request_redraw();
    }
}

/// 若长截图处于可捕获状态且无在途抓屏请求，从滚轮蓄水池/自动节拍中取出一批增量并触发底层滚动抓屏。
pub(super) fn dispatch_pending_long_capture_frame(
    window: &Window,
    state: &mut SnipperState,
    bg_dims: (u32, u32),
) -> bool {
    if state.jobs.has_pending_screen_capture() {
        return false;
    }
    let Some(wheel_delta) = state
        .long_capture
        .as_mut()
        .filter(|session| session.is_capturing && !session.is_editing)
        .and_then(|session| session.take_ready_dispatch_delta(Instant::now()))
    else {
        return false;
    };
    let Some(request_id) = state.jobs.begin_screen_capture() else {
        return false;
    };
    let (focus_point, selection_bounds) = match state.viewport.selection {
        SelectionState::Locked { rect, .. } => {
            let focus = Some((rect.x + rect.width * 0.5, rect.y + rect.height * 0.5));
            let bounds = rect.to_pixel_bounds(bg_dims.0, bg_dims.1);
            let region = (bounds.2 > 0 && bounds.3 > 0).then_some(bounds);
            (focus, region)
        }
        _ => (None, None),
    };

    tracing::debug!(
        "启动长截图滚动抓屏，请求序号: {request_id}，滚动增量: ({:.3}, {:.3})，锚点: {:?}，选区: {:?}",
        wheel_delta.x,
        wheel_delta.y,
        focus_point,
        selection_bounds
    );
    let desktop = state.session.desktop.clone();
    let has_wheel_injection =
        wheel_delta.x.abs() > f32::EPSILON || wheel_delta.y.abs() > f32::EPSILON;
    if desktop.long_capture_requires_window_hide() {
        window.set_visible(false);
    } else if has_wheel_injection {
        if let Err(error) = window.set_cursor_hittest(false) {
            tracing::debug!("禁用覆盖层指针命中失败，回退隐藏窗口: {error}");
            window.set_visible(false);
        } else {
            // Wayland wl_surface.set_input_region 属于双缓冲状态，需触发重绘提交空输入区域。
            window.request_redraw();
        }
    }
    let screen_capture_events = state.jobs.screen_capture_events.clone();
    let mut request = LongCaptureFrameRequest::new(wheel_delta, focus_point);
    if let Some(bounds) = selection_bounds {
        request = request.with_selection_bounds(bounds);
    }
    let submitted = desktop.capture_long_capture_frame(
        request,
        Box::new(move |result| screen_capture_events(request_id, result)),
    );
    if let Err(error) = submitted {
        state.jobs.finish_screen_capture(request_id);
        if let Some(session) = state.long_capture.as_mut() {
            session.stop_auto_scroll();
        }
        let _ = window.set_cursor_hittest(true);
        if !window.is_visible().unwrap_or(false) {
            window.set_visible(true);
        }
        window.focus_window();
        tracing::warn!("提交长截图抓屏任务失败，请求序号: {request_id}，原因: {error}");
        state.set_status_message(format!("提交长截图抓屏任务失败: {error}"));
        window.request_redraw();
        return false;
    }
    true
}

/// 处理鼠标按键并请求重绘。
fn handle_pointer_button(
    context: &mut SnipperDispatchContext<'_>,
    button: winit::event::MouseButton,
    button_state: winit::event::ElementState,
) {
    let need_redraw = handle_mouse_button(context.state, button, button_state, context.bg_image);
    if need_redraw {
        dispatch_pending_long_capture_frame(
            context.window,
            context.state,
            context.bg_image.dimensions(),
        );
        context.window.request_redraw();
    }
}

/// 处理键盘输入并请求重绘。
fn handle_keyboard(context: &mut SnipperDispatchContext<'_>, key_event: &winit::event::KeyEvent) {
    let need_redraw = handle_key_input(
        context.state,
        &key_event.logical_key,
        key_event.state,
        context.bg_image,
    );
    if need_redraw {
        dispatch_pending_long_capture_frame(
            context.window,
            context.state,
            context.bg_image.dimensions(),
        );
        context.window.request_redraw();
    }
}

/// 应用输入法提交或预编辑文本，并同步 IME 光标区域。
fn handle_ime(context: &mut SnipperDispatchContext<'_>, event: Ime) {
    let need_redraw = apply_ime_text(context.state, event);
    if !need_redraw {
        return;
    }
    if let Some(session) = &context.state.editor.annotation.active_text_input {
        context.window.set_ime_cursor_area(
            PhysicalPosition::new(session.pos.x as i32, session.pos.y as i32),
            PhysicalSize::new(24, session.font_size as u32),
        );
    } else if let Some(editor) = &context.state.ocr.editor {
        let layout = editor.layout(
            context.bg_image.width() as f32,
            context.bg_image.height() as f32,
            context.state.editor.annotation.toolbar.ui_scale,
        );
        context.window.set_ime_cursor_area(
            PhysicalPosition::new(layout.body.x as i32, layout.body.y as i32),
            PhysicalSize::new(layout.body.width as u32, 24),
        );
    }
    context.window.request_redraw();
}

/// 将 IME 文本写入当前文本会话；无活动会话时忽略。
fn apply_ime_text(state: &mut SnipperState, event: Ime) -> bool {
    if let Some(editor) = &mut state.ocr.editor {
        match event {
            Ime::Commit(text) => {
                editor.push_commit(&text);
                return true;
            }
            Ime::Preedit(text, _cursor) => {
                editor.set_preedit(&text);
                return true;
            }
            _ => return false,
        }
    }
    let Some(session) = &mut state.editor.annotation.active_text_input else {
        return false;
    };
    match event {
        Ime::Commit(text) => {
            session.push_commit(&text);
            true
        }
        Ime::Preedit(text, _cursor) => {
            session.set_preedit(&text);
            true
        }
        _ => false,
    }
}
