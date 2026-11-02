//! 工作区级剪贴板完成事件路由。

use super::event_handler::finish_snipper_if_ready;
use super::paster_events::remove_paster_instance;
use super::WorkspaceRuntime;
use crate::ports::{ClipboardJobId, ClipboardResult, ClipboardTarget, PortError};

/// 将真实剪贴板结果路由回发起任务的截图覆盖层或贴图窗口。
pub(crate) fn handle_clipboard_finished(
    target: ClipboardTarget,
    job_id: ClipboardJobId,
    result: Result<ClipboardResult, PortError>,
    runtime: &mut WorkspaceRuntime,
) {
    match target {
        ClipboardTarget::Snipper => {
            let Some(view) = runtime.snipper.as_mut() else {
                tracing::debug!("剪贴板任务完成时截图覆盖层已关闭，结果已丢弃");
                return;
            };
            view.handle_clipboard_finished(job_id, result);
            view.request_redraw();
            finish_snipper_if_ready(runtime);
        }
        ClipboardTarget::Paster(paster_id) => {
            let window_id = runtime.instances.iter().find_map(|(window_id, instance)| {
                (instance.item_id == paster_id).then_some(*window_id)
            });
            let Some(window_id) = window_id else {
                tracing::debug!("剪贴板任务完成时贴图窗口已关闭，结果已丢弃: {paster_id}");
                return;
            };
            let should_close = match runtime.instances.get_mut(&window_id) {
                Some(instance) => {
                    let matched = instance.state.handle_clipboard_finished(
                        job_id,
                        result,
                        Some(&instance.window),
                    );
                    if matched {
                        instance.window.request_redraw();
                    }
                    instance.state.should_close
                }
                None => false,
            };
            if should_close {
                remove_paster_instance(runtime, window_id);
            }
        }
    }
}
