//! 贴图集中工作区事件调度与窗口事件响应。

use super::super::window::state::PasterWindowState;
use super::paster_events::handle_paster_window_event;
use super::proxy::{PasterAppEvent, WorkspaceStatus};
use super::window_builder::create_new_paster_window;
use super::WorkspaceRuntime;
use crate::model::{GroupId, PasterId};
use crate::ports::JobId;
use crate::snipper::window::jobs::SnipperJobSink;
use crate::snipper::{SnipperOutcome, SnipperView};
use snapin_core::annotation::shapes::TranslationShapeItem;
use snapin_core::config::Config;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use winit::event::WindowEvent;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::WindowId;

/// 支持的循环轮转贴图最大分组数量
pub const TOTAL_PIN_GROUPS: usize = 4;

/// 判断贴图分组 ID 是否落在有效范围内。
pub fn is_valid_group_id(id: GroupId) -> bool {
    id.get() < TOTAL_PIN_GROUPS
}

/// 处理工作区收到的自定义指令
pub(crate) fn handle_workspace_user_event(
    event: PasterAppEvent,
    runtime: &mut WorkspaceRuntime,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) {
    match event {
        PasterAppEvent::ReloadConfig(new_cfg) => reload_config(*new_cfg, runtime),
        PasterAppEvent::OpenPaster(item) => {
            if let Err(e) = create_new_paster_window(item, runtime, elwt) {
                tracing::error!("在当前事件循环中创建新贴图窗口失败: {e}");
            }
        }
        PasterAppEvent::OpenSnipper(launch) => {
            open_snipper(*launch, runtime, elwt);
        }
        PasterAppEvent::SnipperJobFinished {
            job_id,
            kind,
            outcome,
        } => {
            if let Some(view) = runtime.snipper.as_mut() {
                view.handle_job_finished(job_id, kind, outcome);
                view.request_redraw();
            }
            finish_snipper_if_ready(runtime);
        }
        PasterAppEvent::ToggleVisibility => toggle_group_visibility(runtime),
        PasterAppEvent::SwitchGroup(target) => switch_group(target, runtime),
        PasterAppEvent::GetStatus(responder) => {
            let status = WorkspaceStatus {
                active_group: runtime.manager.active_group(),
                paster_count: runtime.manager.count_in_active_group(),
            };
            let _ = responder.send(status);
        }
        PasterAppEvent::OcrFinished {
            paster_id,
            job_id,
            result,
        } => {
            handle_ocr_finished(paster_id, job_id, result, runtime);
        }
        PasterAppEvent::TranslateFinished {
            paster_id,
            job_id,
            result,
        } => {
            handle_translate_finished(paster_id, job_id, result, runtime);
        }
        PasterAppEvent::SaveFinished {
            paster_id,
            job_id,
            result,
        } => {
            handle_save_finished(paster_id, job_id, result, runtime);
        }
        PasterAppEvent::Shutdown(responder) => shutdown_workspace(runtime, responder, elwt),
    }
}

/// 热更新工作区配置并同步给所有贴图窗口。
fn reload_config(new_config: Config, runtime: &mut WorkspaceRuntime) {
    runtime.cfg = Arc::new(new_config);
    for inst in runtime.instances.values_mut() {
        inst.state.config = runtime.cfg.clone();
    }
    tracing::info!("集中贴图工作区已完成全局配置热更新");
}

/// 切换当前分组全部贴图的显隐。
fn toggle_group_visibility(runtime: &mut WorkspaceRuntime) {
    let new_visible = runtime.manager.toggle_current_group_visibility();
    let active = runtime.manager.active_group();
    for inst in runtime.instances.values_mut() {
        if inst.state.item.group_id == active {
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

/// 切换活动分组，同步窗口可见性并持久化分组选择。
fn switch_group(target: Option<GroupId>, runtime: &mut WorkspaceRuntime) {
    let next_group = match target {
        Some(id) if is_valid_group_id(id) => id,
        Some(id) => {
            tracing::warn!(
                "忽略非法贴图分组 ID: {id}，有效范围: 0..={}",
                TOTAL_PIN_GROUPS.saturating_sub(1)
            );
            return;
        }
        None => GroupId::new((runtime.manager.active_group().get() + 1) % TOTAL_PIN_GROUPS),
    };
    runtime.manager.switch_group(next_group);
    let group_label = format!(
        "当前贴图组: {}/{}（ID {next_group}）",
        next_group.get().saturating_add(1),
        TOTAL_PIN_GROUPS
    );
    for inst in runtime.instances.values_mut() {
        let visible = inst.state.item.group_id == next_group;
        inst.window.set_visible(visible);
        if visible {
            inst.state.set_status_message(group_label.clone());
            inst.window.request_redraw();
        }
    }
    persist_active_group(runtime, next_group.get());
    tracing::info!("已切换活动贴图分组至: {next_group}");
}

/// 平滑关闭截图覆盖层、全部贴图窗口与工作区事件循环。
fn shutdown_workspace(
    runtime: &mut WorkspaceRuntime,
    responder: Option<tokio::sync::oneshot::Sender<()>>,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) {
    if let Some(mut view) = runtime.snipper.take() {
        view.cancel();
    }
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

/// 在现有事件循环中创建截图覆盖层。
fn open_snipper(
    launch: crate::snipper::SnipperLaunch,
    runtime: &mut WorkspaceRuntime,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) {
    if let Some(mut previous) = runtime.snipper.take() {
        previous.cancel();
        tracing::warn!("已有截图覆盖层未结束，已取消旧会话并打开新会话");
    }

    // EventLoopProxy 在 macOS 上为 Send 但不是 Sync；媒体任务线程需要共享回调，
    // 因此用短临界区互斥锁包裹代理，既满足跨线程约束又只在投递结果时短暂持锁。
    let event_proxy = Arc::new(Mutex::new(runtime.event_proxy.clone()));
    let sink: SnipperJobSink = Arc::new(move |job_id, kind, outcome| match event_proxy.lock() {
        Ok(proxy) => {
            let _ = proxy.send_event(PasterAppEvent::SnipperJobFinished {
                job_id,
                kind,
                outcome,
            });
        }
        Err(_) => tracing::warn!("截图事件代理锁中毒，媒体任务结果未能回传"),
    });

    match SnipperView::new(elwt, launch, sink) {
        Ok(view) => {
            tracing::info!("截图覆盖层已挂载至常驻工作区事件循环");
            runtime.snipper = Some(view);
        }
        Err(error) => {
            tracing::error!("创建截图覆盖层失败: {error}");
            notify_snipper_finished(runtime, SnipperOutcome::Cancelled);
        }
    }
}

/// 若截图覆盖层已产出结果，则销毁窗口并回传应用层。
fn finish_snipper_if_ready(runtime: &mut WorkspaceRuntime) {
    let Some(outcome) = runtime
        .snipper
        .as_mut()
        .and_then(|view| view.take_outcome())
    else {
        return;
    };
    if let Some(mut view) = runtime.snipper.take() {
        view.cancel();
    }
    notify_snipper_finished(runtime, outcome);
}

/// 投递截图结果；回调由应用层负责转换为有界通道消息。
fn notify_snipper_finished(runtime: &WorkspaceRuntime, outcome: SnipperOutcome) {
    if let Some(callback) = &runtime.on_snipper_finished {
        callback(outcome);
    }
}

/// 将活动分组写入运行时状态文件，保证守护进程重启后恢复上次分组。
fn persist_active_group(runtime: &mut WorkspaceRuntime, group: usize) {
    let state = crate::ports::WorkspaceState {
        active_group_id: group,
    };
    if let Err(error) = runtime.state_store.save(state) {
        tracing::warn!("持久化贴图活动分组失败: {error}");
    }
}

/// 处理特定窗口触发的 Winit 窗口事件
pub(crate) fn handle_workspace_window_event(
    window_id: WindowId,
    event: WindowEvent,
    runtime: &mut WorkspaceRuntime,
) {
    if runtime
        .snipper
        .as_ref()
        .is_some_and(|view| view.window_id() == window_id)
    {
        if let Some(view) = runtime.snipper.as_mut() {
            view.dispatch(event);
        }
        finish_snipper_if_ready(runtime);
        return;
    }

    handle_paster_window_event(window_id, event, runtime);
}

/// 处理贴图保存结果：回填文件路径与状态提示。
///
/// 保存通知与“保存后打开目录”由应用层媒体执行器完成，UI 只负责更新窗口状态，
/// 避免表现层重复解释通知配置。
fn handle_save_finished(
    paster_id: PasterId,
    job_id: JobId,
    result: Result<Option<PathBuf>, String>,
    runtime: &mut WorkspaceRuntime,
) {
    let Some(inst) = runtime
        .instances
        .values_mut()
        .find(|inst| inst.item_id == paster_id)
    else {
        tracing::warn!("保存结果到达时目标贴图已关闭，结果已丢弃: {paster_id}");
        return;
    };

    if !inst.state.finish_job(job_id) {
        tracing::debug!("丢弃过期的贴图保存结果: {job_id:?}");
        return;
    }
    match result {
        Ok(Some(path)) => {
            inst.state.file_path = Some(path.clone());
            inst.state
                .set_status_message(format!("图像已保存至: {}", path.display()));
        }
        Ok(None) => inst.state.set_status_message("已取消保存"),
        Err(message) => {
            tracing::warn!("贴图保存失败: {message}");
            inst.state
                .set_status_message(format!("保存失败: {message}"));
        }
    }

    inst.window.request_redraw();
}

/// 将后台 OCR 结果回填到发起任务的贴图窗口。
fn handle_ocr_finished(
    paster_id: PasterId,
    job_id: JobId,
    result: Result<String, String>,
    runtime: &mut WorkspaceRuntime,
) {
    let Some(inst) = runtime
        .instances
        .values_mut()
        .find(|inst| inst.item_id == paster_id)
    else {
        tracing::debug!("文字识别结果到达时目标贴图已关闭，结果已丢弃: {paster_id}");
        return;
    };

    if !inst.state.finish_job(job_id) {
        tracing::debug!("丢弃过期的贴图识别结果: {job_id:?}");
        return;
    }
    apply_ocr_result(&mut inst.state, result);
    inst.window.request_redraw();
}

/// 将后台翻译结果注入发起任务的贴图窗口标注历史并重建展示图像。
fn handle_translate_finished(
    paster_id: PasterId,
    job_id: JobId,
    result: Result<Vec<TranslationShapeItem>, String>,
    runtime: &mut WorkspaceRuntime,
) {
    let Some(inst) = runtime
        .instances
        .values_mut()
        .find(|inst| inst.item_id == paster_id)
    else {
        tracing::debug!("翻译结果到达时目标贴图已关闭，结果已丢弃: {paster_id}");
        return;
    };

    if !inst.state.finish_job(job_id) {
        tracing::debug!("丢弃过期的贴图翻译结果: {job_id:?}");
        return;
    }
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
            match state.desktop.write_text_clipboard(text) {
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

    /// 分组 ID 必须落在 0..TOTAL_PIN_GROUPS，边界值不可越界
    #[test]
    fn test_group_id_validation() {
        assert!(is_valid_group_id(0.into()));
        assert!(is_valid_group_id((TOTAL_PIN_GROUPS - 1).into()));
        assert!(!is_valid_group_id(TOTAL_PIN_GROUPS.into()));
        assert!(!is_valid_group_id(usize::MAX.into()));
    }
}
