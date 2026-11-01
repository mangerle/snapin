//! 守护进程后台版本巡检。

use snapin_platform::send_desktop_notification;
use snapin_runtime::CancellationToken;
use std::time::Duration;

/// 守护进程后台更新轻量巡检循环（模式 B：仅提示不主动下载）。
pub(super) async fn run_update_check_loop(
    custom_url: Option<String>,
    interval_hours: u32,
    cancel: CancellationToken,
) {
    let period = Duration::from_secs((interval_hours as u64).saturating_mul(3600));
    if wait_or_cancel(&cancel, Duration::from_secs(10)).await {
        return;
    }

    loop {
        tracing::debug!("后台轻量检查 Snapin 新版本...");
        let check = snapin_updater::check_update_async(custom_url.as_deref());
        let check_result = tokio::select! {
            _ = cancel.cancelled() => return,
            result = check => result,
        };
        match check_result {
            Ok(Some(update)) => {
                let new_version = update.version().to_string();
                tracing::info!("检测到 Snapin 新版本: v{new_version}");
                let body =
                    format!("检测到新版本 v{new_version}，可前往设置中心【关于】页查看并更新。");
                if let Err(error) = send_desktop_notification("Snapin 发现新版本", &body).await
                {
                    tracing::debug!("发送版本更新桌面通知失败: {error}");
                }
            }
            Ok(None) => tracing::debug!("当前已是最新版本，无需更新。"),
            Err(error) => tracing::debug!("后台静默检查更新失败（已安全忽略）: {error}"),
        }
        if wait_or_cancel(&cancel, period).await {
            return;
        }
    }
}

/// 等待指定时长或取消信号；返回 `true` 表示已取消。
async fn wait_or_cancel(cancel: &CancellationToken, duration: Duration) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => true,
        _ = tokio::time::sleep(duration) => false,
    }
}
