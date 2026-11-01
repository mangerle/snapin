//! 配置热重载指令。

use crate::services::AppServices;
use snapin_core::config;
use snapin_daemon::DaemonResponse;
use snapin_ui::paster::PasterWorkspaceProxy;

pub(super) async fn reload_config(
    workspace: &PasterWorkspaceProxy,
    services: &AppServices,
) -> DaemonResponse {
    let loaded = tokio::task::spawn_blocking(|| config::load_or_create_config(None)).await;

    let new_config = match loaded {
        Ok(Ok(config)) => config,
        Ok(Err(error)) => {
            tracing::error!("重新加载配置文件失败: {error}");
            return DaemonResponse::Error(format!("重新加载配置文件失败: {error}"));
        }
        Err(error) => {
            tracing::error!("配置加载任务异常: {error}");
            return DaemonResponse::Error(format!("配置加载任务异常: {error}"));
        }
    };

    let hotkeys = new_config.hotkeys.clone();
    let hotkey_result =
        tokio::task::spawn_blocking(move || snapin_platform::sync_hotkeys_to_gnome(&hotkeys)).await;
    match hotkey_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("热重载配置时同步系统快捷键失败: {error}"),
        Err(error) => tracing::warn!("快捷键同步任务异常: {error}"),
    }

    services.update_config(new_config.clone());
    match workspace.reload_config(new_config) {
        Ok(()) => {
            tracing::info!("守护进程已成功热重载配置并同步至贴图工作区、媒体服务与系统快捷键");
            DaemonResponse::Ok
        }
        Err(error) => {
            tracing::warn!("热重载配置时同步贴图工作区失败: {error}");
            DaemonResponse::Error(format!("同步贴图工作区失败: {error}"))
        }
    }
}
