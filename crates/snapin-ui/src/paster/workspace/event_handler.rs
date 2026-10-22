//! 贴图集中工作区事件调度与窗口事件响应。

use super::super::window::events::{
    handle_paster_cursor_move, handle_paster_key, handle_paster_mouse_button, handle_paster_scroll,
};
use super::super::window::render::render_paster_to_surface;
use super::super::window::state::PasterWindowState;
use super::super::window::text_session::{refresh_ime_cursor_area, sync_ime_state};
use super::proxy::{PasterAppEvent, WorkspaceStatus};
use super::window_builder::create_new_paster_window;
use super::WorkspaceRuntime;
use snapin_core::annotation::shapes::TranslationShapeItem;
use snapin_core::config::{self, Config};
use snapin_platform::{send_desktop_notification, write_text_to_clipboard};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::event::{Ime, WindowEvent};
use winit::event_loop::EventLoopWindowTarget;
use winit::window::WindowId;

/// 支持的循环轮转贴图最大分组数量
pub const TOTAL_PIN_GROUPS: usize = 4;

/// 处理工作区收到的自定义指令
pub(crate) fn handle_workspace_user_event(
    event: PasterAppEvent,
    runtime: &mut WorkspaceRuntime,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) {
    match event {
        PasterAppEvent::ReloadConfig(new_cfg) => {
            runtime.cfg = Arc::new(*new_cfg);
            runtime.jobs.update_config(runtime.cfg.clone());
            for inst in runtime.instances.values_mut() {
                inst.state.config = runtime.cfg.clone();
            }
            tracing::info!("集中贴图工作区已完成全局配置热更新");
        }
        PasterAppEvent::OpenPaster(item) => {
            if let Err(e) = create_new_paster_window(item, runtime, elwt) {
                tracing::error!("在当前事件循环中创建新贴图窗口失败: {e}");
            }
        }
        PasterAppEvent::ToggleVisibility => {
            let new_visible = runtime.manager.toggle_current_group_visibility();
            let active = runtime.manager.active_group();
            for inst in runtime.instances.values_mut() {
                if inst.state.item.group_id == active {
                    inst.state.item.is_visible = new_visible;
                    inst.window.set_visible(new_visible);
                }
            }
            tracing::info!(
                "已切换当前贴图组 ({active}) 显隐状态: {}",
                if new_visible {
                    "全部显示"
                } else {
                    "全部隐藏"
                }
            );
        }
        PasterAppEvent::SwitchGroup(target) => {
            let next_group = match target {
                Some(id) => id,
                None => (runtime.manager.active_group() + 1) % TOTAL_PIN_GROUPS,
            };
            runtime.manager.switch_group(next_group);
            for inst in runtime.instances.values_mut() {
                let visible = inst.state.item.group_id == next_group;
                inst.state.item.is_visible = visible;
                inst.window.set_visible(visible);
            }
            persist_active_group(runtime, next_group);
            tracing::info!("已切换活动贴图分组至: {next_group}");
        }
        PasterAppEvent::GetStatus(responder) => {
            let status = WorkspaceStatus {
                active_group: runtime.manager.active_group(),
                paster_count: runtime.manager.count_in_active_group(),
            };
            let _ = responder.send(status);
        }
        PasterAppEvent::OcrFinished(result) => handle_ocr_finished(result, runtime),
        PasterAppEvent::TranslateFinished(result) => handle_translate_finished(result, runtime),
        PasterAppEvent::SaveAsFinished { paster_id, result } => {
            handle_save_as_finished(paster_id, result, runtime);
        }
        PasterAppEvent::Shutdown(responder) => {
            // 工作区停机时逐个结束会话并关闭 IME，再统一释放窗口实例
            for inst in runtime.instances.values_mut() {
                inst.state.cancel_text_session();
                inst.window.set_ime_allowed(false);
            }
            runtime.instances.clear();
            if let Some(responder) = responder {
                let _ = responder.send(());
            }
            elwt.exit();
        }
    }
}

/// 将活动分组写回配置文件，保证守护进程重启后恢复到上次分组。
///
/// 设计取舍：配置文件仅数 KB，切换分组是低频用户动作，这里同步写盘以省去
/// 额外的异步通道与停机落盘逻辑；写失败只告警，不影响本次分组切换结果。
fn persist_active_group(runtime: &mut WorkspaceRuntime, group: usize) {
    let config = config_with_active_group(&runtime.cfg, group);
    let updated = Arc::new(config.clone());
    runtime.cfg = updated.clone();
    for inst in runtime.instances.values_mut() {
        inst.state.config = updated.clone();
    }

    let path = match config::get_default_config_path() {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!("定位配置文件路径失败，无法持久化贴图活动分组: {error}");
            return;
        }
    };
    if let Err(error) = config::save_config(&config, &path) {
        tracing::warn!("持久化贴图活动分组失败: {error}");
    }
}

/// 生成一份仅更新活动分组字段的配置副本。
fn config_with_active_group(config: &Config, group: usize) -> Config {
    let mut updated = config.clone();
    updated.paster.active_group_id = group;
    updated
}

/// 处理特定窗口触发的 Winit 窗口事件
pub(crate) fn handle_workspace_window_event(
    window_id: WindowId,
    event: WindowEvent,
    runtime: &mut WorkspaceRuntime,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) {
    let mut should_close = false;

    if let Some(inst) = runtime.instances.get_mut(&window_id) {
        match event {
            WindowEvent::CloseRequested => {
                should_close = true;
            }
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
                if inst.state.should_close {
                    should_close = true;
                }
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
                if inst.state.should_close {
                    should_close = true;
                }
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
    }

    if should_close {
        if let Some(mut removed) = runtime.instances.remove(&window_id) {
            // 关闭前结束文字会话并显式关闭 IME，避免输入法候选框残留在桌面上
            removed.state.cancel_text_session();
            removed.window.set_ime_allowed(false);
            runtime.manager.remove_paster(removed.item_id);
            tracing::info!(
                "贴图已安全关闭: {}，工作区剩余活动贴图: {}",
                removed.item_id,
                runtime.instances.len()
            );
        }
        if runtime.exit_on_all_closed && runtime.instances.is_empty() {
            tracing::info!("所有单次会话贴图窗口均已关闭，平滑退出工作区");
            elwt.exit();
        }
    }
}

/// 事件循环空闲时清理过期提示，并返回最近一条提示的失效时刻。
///
/// 返回 `Some(deadline)` 时调用方应把控制流切换为定时等待，确保提示到期后
/// 即使没有任何输入事件也能被清除并触发一次重绘；返回 `None` 时恢复无限等待。
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
    }
    earliest
}

/// 处理“另存为”对话框结果：回填文件路径、状态提示与保存通知。
fn handle_save_as_finished(
    paster_id: u64,
    result: Result<Option<PathBuf>, String>,
    runtime: &mut WorkspaceRuntime,
) {
    let Some(inst) = runtime
        .instances
        .values_mut()
        .find(|inst| inst.item_id == paster_id)
    else {
        tracing::warn!("另存为结果到达时目标贴图已关闭，结果已丢弃: {paster_id}");
        return;
    };

    match result {
        Ok(Some(path)) => {
            inst.state.file_path = Some(path.clone());
            inst.state
                .set_status_message(format!("图像已另存至: {}", path.display()));
            if inst.state.config.output.notify_on_save {
                let message = format!("贴图图像已保存至: {}", path.display());
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let _ = send_desktop_notification("Snapin 贴图已保存", &message).await;
                    });
                }
            }
        }
        Ok(None) => inst.state.set_status_message("已取消另存为"),
        Err(message) => {
            tracing::warn!("贴图另存为失败: {message}");
            inst.state
                .set_status_message(format!("另存为失败: {message}"));
        }
    }

    inst.window.request_redraw();
}

/// 将后台 OCR 结果回填到唯一处于等待状态的贴图窗口。
fn handle_ocr_finished(result: Result<String, String>, runtime: &mut WorkspaceRuntime) {
    runtime.jobs.complete();
    let Some(inst) = runtime
        .instances
        .values_mut()
        .find(|inst| inst.state.job_pending)
    else {
        tracing::debug!("文字识别结果到达时没有等待中的贴图窗口，结果已丢弃");
        return;
    };

    inst.state.job_pending = false;
    apply_ocr_result(&mut inst.state, result);
    inst.window.request_redraw();
}

/// 将后台翻译结果注入标注历史并重建展示图像。
fn handle_translate_finished(
    result: Result<Vec<TranslationShapeItem>, String>,
    runtime: &mut WorkspaceRuntime,
) {
    runtime.jobs.complete();
    let Some(inst) = runtime
        .instances
        .values_mut()
        .find(|inst| inst.state.job_pending)
    else {
        tracing::debug!("翻译结果到达时没有等待中的贴图窗口，结果已丢弃");
        return;
    };

    inst.state.job_pending = false;
    apply_translate_result(&mut inst.state, result);
    inst.window.request_redraw();
}

/// 处理整图 OCR 结果：成功写入剪贴板并提示字符数，失败与空结果给出中文提示。
fn apply_ocr_result(state: &mut PasterWindowState, result: Result<String, String>) {
    match result {
        Ok(text) => {
            let text = text.trim();
            if text.is_empty() {
                state.set_status_message("未识别到文字");
                return;
            }
            match write_text_to_clipboard(text) {
                Ok(()) => {
                    let chars = text.chars().count();
                    state.set_status_message(format!("已复制识别文字（{chars} 字符）"));
                    tracing::info!("贴图整图文字识别完成，已复制 {chars} 个字符到剪贴板");
                }
                Err(error) => {
                    tracing::warn!("复制贴图识别文字到系统剪贴板失败: {error}");
                    state.set_status_message(format!("复制识别文字失败: {error}"));
                }
            }
        }
        Err(message) => {
            tracing::warn!("贴图整图文字识别失败: {message}");
            state.set_status_message(format!("文字识别失败: {message}"));
        }
    }
}

/// 处理整图翻译结果：作为一个复合标注注入历史，空结果视为未识别到文字。
fn apply_translate_result(
    state: &mut PasterWindowState,
    result: Result<Vec<TranslationShapeItem>, String>,
) {
    match result {
        Ok(items) => {
            if items.is_empty() {
                state.set_status_message("未识别到文字");
                return;
            }
            let count = state.apply_translation_items(items);
            state.set_status_message(format!("已翻译 {count} 段文字"));
            tracing::info!("贴图整图翻译完成，共注入 {count} 段译文");
        }
        Err(message) => {
            tracing::warn!("贴图整图翻译失败: {message}");
            state.set_status_message(format!("翻译失败: {message}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 更新活动分组时不得改动其它配置字段，也不得反向污染原配置
    #[test]
    fn test_config_with_active_group_preserves_other_fields() {
        let mut config = Config::default();
        config.paster.initial_opacity = 0.5;
        config.paster.active_group_id = 0;

        let updated = config_with_active_group(&config, 2);

        assert_eq!(updated.paster.active_group_id, 2);
        assert_eq!(updated.paster.initial_opacity, 0.5);
        assert_eq!(config.paster.active_group_id, 0);
    }
}
