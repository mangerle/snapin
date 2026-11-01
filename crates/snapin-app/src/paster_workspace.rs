//! 贴图工作区启动适配：GUI 线程派生、配置快照与端口装配。
//!
//! 这些职责原先散落在 `snapin-ui`，使表现层在内部读取配置文件并创建线程。
//! 现在 UI 只提供事件循环与窗口运行时，应用层负责决定在哪个线程启动、
//! 使用哪份配置以及注入哪些端口。

use crate::services::AppServices;
use snapin_ui::paster::{
    create_paster_workspace, run_paster_workspace_loop, PasterItem, PasterWorkspaceOptions,
    PasterWorkspaceProxy,
};
use snapin_ui::UiError;
use std::sync::Arc;

/// 在专用 GUI 线程中拉起常驻贴图工作区，并返回线程安全代理。
///
/// # Errors
///
/// GUI 线程派生失败或事件循环启动失败时返回 [`UiError::Window`]。
pub fn spawn_paster_workspace_thread(
    services: &Arc<AppServices>,
    exit_on_all_closed: bool,
) -> Result<PasterWorkspaceProxy, UiError> {
    let options = workspace_options(services, exit_on_all_closed);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("snapin-paster-gui".to_string())
        .spawn(move || {
            let (event_loop, proxy) = match create_paster_workspace() {
                Ok(pair) => pair,
                Err(error) => {
                    tracing::error!("创建集中贴图工作区失败: {error}");
                    return;
                }
            };
            let _ = tx.send(proxy);
            if let Err(error) = run_paster_workspace_loop(event_loop, options) {
                tracing::error!("集中贴图工作区主循环异常退出: {error}");
            }
        })
        .map_err(|error| UiError::Window {
            message: format!("派生贴图工作区 GUI 线程失败: {error}"),
        })?;

    rx.recv().map_err(|_| UiError::Window {
        message: "等待集中贴图工作区事件循环就绪失败".to_string(),
    })
}

/// 在当前线程运行一次贴图工作区会话，直到最后一个贴图关闭。
///
/// 调用方必须把本函数放在允许创建事件循环的 GUI 线程中执行；
/// 应用层在单次贴图模式下通过阻塞线程池调用。
///
/// # Errors
///
/// 事件循环创建或运行失败时返回 [`UiError::Window`]。
pub fn run_paster_window_session(
    services: &Arc<AppServices>,
    item: PasterItem,
) -> Result<(), UiError> {
    let options = workspace_options(services, true);
    let (event_loop, proxy) = create_paster_workspace()?;
    proxy.open_paster(item)?;
    run_paster_workspace_loop(event_loop, options)
}

/// 从应用服务容器组装工作区启动参数。
fn workspace_options(
    services: &Arc<AppServices>,
    exit_on_all_closed: bool,
) -> PasterWorkspaceOptions {
    PasterWorkspaceOptions {
        config: (*services.config()).clone(),
        desktop: services.desktop(),
        media: services.media(),
        config_store: services.config_store(),
        exit_on_all_closed,
    }
}
