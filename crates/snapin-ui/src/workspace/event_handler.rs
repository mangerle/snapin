//! 贴图集中工作区事件调度与窗口事件响应。

use super::clipboard_events::handle_clipboard_finished;
use super::job_events::handle_paster_job_finished;
use super::paster_events::handle_paster_window_event;
use super::persistence::persist_active_group;
use super::proxy::{PasterAppEvent, WorkspaceStatus};
use super::window_builder::create_new_paster_window;
use super::WorkspaceRuntime;
use crate::model::GroupId;
use crate::snipper::window::jobs::SnipperJobSink;
use crate::snipper::window::{SnipperView, SnipperViewInit};
use crate::snipper::SnipperOutcome;
use snapin_config::config::Config;
use std::sync::{Arc, Mutex};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
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
    elwt: &ActiveEventLoop,
) {
    match event {
        PasterAppEvent::ReloadConfig(new_cfg) => reload_config(*new_cfg, runtime),
        PasterAppEvent::OpenPaster(item) => {
            if let Err(e) = create_new_paster_window(item, runtime, elwt) {
                tracing::warn!("在当前事件循环中创建新贴图窗口失败: {e}");
            }
        }
        PasterAppEvent::OpenSnipper(launch) => {
            open_snipper(*launch, runtime, elwt);
        }
        PasterAppEvent::ClipboardFinished {
            target,
            job_id,
            result,
        } => handle_clipboard_finished(target, job_id, result, runtime),
        PasterAppEvent::SnipperRecordingEvent { event } => {
            if let Some(view) = runtime.snipper.as_mut() {
                view.handle_recording_event(event);
                view.request_redraw();
            }
            sync_snipper_ui_ticker(runtime);
            finish_snipper_if_ready(runtime);
        }
        PasterAppEvent::SnipperUiTick => handle_snipper_ui_tick(runtime),
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
        PasterAppEvent::HostWake => {
            if let Some(on_host_wake) = runtime.on_host_wake.as_mut() {
                on_host_wake();
            }
        }
        PasterAppEvent::ToggleVisibility => toggle_group_visibility(runtime),
        PasterAppEvent::ShowWorkspace => show_workspace(runtime),
        PasterAppEvent::SwitchGroup(target) => switch_group(target, runtime),
        PasterAppEvent::GetStatus(responder) => {
            let status = WorkspaceStatus {
                active_group: runtime.manager.active_group(),
                paster_count: runtime.manager.count_in_active_group(),
            };
            let _ = responder.send(status);
        }
        PasterAppEvent::PasterJobFinished {
            paster_id,
            job_id,
            kind,
            outcome,
        } => {
            handle_paster_job_finished(paster_id, job_id, kind, outcome, runtime);
        }
        PasterAppEvent::Shutdown(responder) => shutdown_workspace(runtime, responder, elwt),
    }
}

/// 热更新工作区配置并同步给所有贴图窗口。
fn reload_config(new_config: Config, runtime: &mut WorkspaceRuntime) {
    runtime.cfg = Arc::new(new_config);
    for inst in runtime.instances.values_mut() {
        inst.state.services.config = runtime.cfg.clone();
    }
    tracing::info!("集中贴图工作区已完成全局配置热更新");
}

/// 切换当前分组全部贴图的显隐。
fn toggle_group_visibility(runtime: &mut WorkspaceRuntime) {
    let new_visible = runtime.manager.toggle_current_group_visibility();
    let active = runtime.manager.active_group();
    for inst in runtime.instances.values_mut() {
        if inst.state.document.item.group_id == active {
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

/// 强制显示当前活动分组，作为“打开贴图工作区”的确定行为。
fn show_workspace(runtime: &mut WorkspaceRuntime) {
    runtime.manager.show_current_group();
    let active = runtime.manager.active_group();
    for inst in runtime.instances.values_mut() {
        let visible = inst.state.document.item.group_id == active;
        inst.window.set_visible(visible);
        if visible {
            inst.window.request_redraw();
        }
    }
    tracing::info!("已打开贴图工作区并使当前分组可见: {active}");
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
        let visible = inst.state.document.item.group_id == next_group;
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
    elwt: &ActiveEventLoop,
) {
    if let Some(mut view) = runtime.snipper.take() {
        view.cancel();
    }
    // 工作区停机时逐个结束会话并关闭 IME，再统一释放窗口实例
    for inst in runtime.instances.values_mut() {
        inst.state.cancel_pending_job();
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
    elwt: &ActiveEventLoop,
) {
    runtime.ui_ticker = None;
    if let Some(mut previous) = runtime.snipper.take() {
        previous.cancel();
        tracing::warn!("已有截图覆盖层未结束，已取消旧会话并打开新会话");
    }

    // EventLoopProxy 在 macOS 上为 Send 但不是 Sync；媒体任务线程需要共享回调，
    // 因此用短临界区互斥锁包裹代理，既满足跨线程约束又只在投递结果时短暂持锁。
    let event_proxy = Arc::new(Mutex::new(runtime.event_proxy.clone()));
    let sink: SnipperJobSink = Arc::new(move |job_id, kind, outcome| {
        let proxy = event_proxy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = proxy.send_event(PasterAppEvent::SnipperJobFinished {
            job_id,
            kind,
            outcome,
        });
    });

    let event_proxy = Arc::new(Mutex::new(runtime.event_proxy.clone()));
    let recording_sink: crate::ports::RecordingEventSink = Arc::new(move |event| {
        let proxy = event_proxy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = proxy.send_event(PasterAppEvent::SnipperRecordingEvent { event });
    });
    let init = SnipperViewInit {
        launch,
        job_sink: sink,
        recording_sink,
        clipboard_events: runtime.clipboard_events.clone(),
    };
    match SnipperView::new(elwt, init) {
        Ok(view) => {
            tracing::info!("截图覆盖层已挂载至常驻工作区事件循环");
            runtime.snipper = Some(view);
        }
        Err(error) => {
            tracing::warn!("创建截图覆盖层失败: {error}");
            notify_snipper_finished(runtime, SnipperOutcome::Cancelled);
        }
    }
}

/// 若截图覆盖层已产出结果，则销毁窗口并回传应用层。
pub(crate) fn finish_snipper_if_ready(runtime: &mut WorkspaceRuntime) {
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
    runtime.ui_ticker = None;
    notify_snipper_finished(runtime, outcome);
}

/// 推进录制秒表或结果页预览，并同步受管定时线程的生命周期。
fn handle_snipper_ui_tick(runtime: &mut WorkspaceRuntime) {
    let now = std::time::Instant::now();
    let mut redraw = false;
    if let Some(view) = runtime.snipper.as_mut() {
        redraw |= view.refresh_recording_elapsed(now);
        redraw |= view.advance_recording_preview(now);
    }
    if redraw {
        if let Some(view) = runtime.snipper.as_ref() {
            view.request_redraw();
        }
    }
    sync_snipper_ui_ticker(runtime);
}

/// 根据当前录屏阶段启动或回收秒表与预览定时线程。
fn sync_snipper_ui_ticker(runtime: &mut WorkspaceRuntime) {
    let should_run = runtime
        .snipper
        .as_ref()
        .is_some_and(SnipperView::needs_periodic_ui_tick);
    match (should_run, runtime.ui_ticker.is_some()) {
        (true, false) => match super::RecordingUiTicker::start(runtime.event_proxy.clone()) {
            Ok(ticker) => runtime.ui_ticker = Some(ticker),
            Err(error) => tracing::warn!("{error}"),
        },
        (false, true) => runtime.ui_ticker = None,
        (true, true) | (false, false) => {}
    }
}

/// 投递截图结果；回调由应用层负责转换为有界通道消息。
fn notify_snipper_finished(runtime: &WorkspaceRuntime, outcome: SnipperOutcome) {
    if let Some(callback) = &runtime.on_snipper_finished {
        callback(outcome);
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
