//! 截图流水线调度与呈现模块。

use anyhow::{Context, Result};
use image::RgbaImage;
use snapin_core::config::Config;
use snapin_platform::{
    capture_fullscreen_extension, capture_fullscreen_portal, generate_secure_temp_path,
    send_desktop_notification,
};
use snapin_ui::snipper::{run_snipper_session, SnipMode, SnipperOutcome};
use std::time::Instant;

/// 获取屏幕快照（优先扩展零白闪通道，自动优雅降级 Portal）
pub async fn acquire_screen_snapshot() -> Result<RgbaImage> {
    let t_start = Instant::now();
    let target_path = generate_secure_temp_path("screen", "png");
    let target_str = target_path.to_string_lossy().to_string();

    let t_dbus_start = Instant::now();
    if let Ok(true) = capture_fullscreen_extension(&target_str).await {
        let t_dbus_end = Instant::now();
        tracing::info!(
            "命中 GNOME 扩展原生零白闪截屏通道，D-Bus 耗时: {:.2}ms",
            (t_dbus_end - t_dbus_start).as_secs_f64() * 1000.0
        );

        let t_decode_start = Instant::now();
        if let Ok(bytes) = std::fs::read(&target_path) {
            let _ = std::fs::remove_file(&target_path);
            if let Ok(dynamic_img) = image::load_from_memory(&bytes) {
                let rgba = dynamic_img.into_rgba8();
                let t_decode_end = Instant::now();
                tracing::info!(
                    "PNG 读取并内存解码耗时: {:.2}ms, 原生全屏截屏总耗时: {:.2}ms",
                    (t_decode_end - t_decode_start).as_secs_f64() * 1000.0,
                    (t_decode_end - t_start).as_secs_f64() * 1000.0
                );
                return Ok(rgba);
            }
        }
    }

    tracing::info!(
        "当前走 XDG Desktop Portal 通道 (注销重新登录桌面后将自动激活 GNOME 原生零白闪通道)"
    );
    let rgba = capture_fullscreen_portal()
        .await
        .context("Portal 全屏截图捕获失败")?;
    Ok(rgba)
}

/// 执行单次截图流程
pub async fn run_oneshot_snip(cfg: Config, mode: SnipMode) -> Result<()> {
    let t0 = Instant::now();
    let img = acquire_screen_snapshot().await?;
    let t1 = Instant::now();
    tracing::info!(
        "快照准备就绪: {}x{}，耗时: {:.2}ms，正在瞬间呈现交互窗口...",
        img.width(),
        img.height(),
        (t1 - t0).as_secs_f64() * 1000.0
    );

    let cfg_clone = cfg.clone();
    let outcome = tokio::task::spawn_blocking(move || run_snipper_session(img, cfg_clone, mode))
        .await
        .context("截图任务调度异常")?
        .context("截图交互会话运行失败")?;

    if let Some(outcome) = outcome {
        match outcome {
            SnipperOutcome::Copied(_) => {
                if cfg.output.notify_on_copy {
                    send_desktop_notification("Snapin 截图", "选区图像已成功复制到剪贴板").await?;
                }
            }
            SnipperOutcome::Saved(path) => {
                if cfg.output.notify_on_save {
                    let msg = format!("选区图像已保存至: {}", path.display());
                    send_desktop_notification("Snapin 截图已保存", &msg).await?;
                }
            }
            SnipperOutcome::Pinned(item) => {
                tracing::info!("正在原地挂载选区贴图置顶窗口...");
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = snapin_ui::paster::spawn_paster_window(item) {
                        tracing::error!("贴图窗口运行异常: {e}");
                    }
                })
                .await
                .context("贴图任务调度异常")?;
            }
            SnipperOutcome::Cancelled => {
                tracing::info!("用户已取消本次截图");
            }
        }
    }

    Ok(())
}
