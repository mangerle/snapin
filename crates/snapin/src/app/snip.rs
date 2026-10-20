//! 截图流水线调度与呈现模块。

use anyhow::{Context, Result};
use image::RgbaImage;
use snapin_core::config::Config;
use snapin_core::AppPaths;
use snapin_platform::{
    capture_fullscreen_portal, get_platform_capabilities, send_desktop_notification,
};
use snapin_ui::snipper::{run_snipper_session, SnipMode, SnipperOutcome};
use std::time::Instant;

/// 获取屏幕快照。
///
/// 通道选择说明：本项目早期在配套 GNOME 扩展内直接读取 ClutterStage 原始像素以求"零白闪极速"，
/// 但该调用在 GNOME 50 的 GJS 上会因数组返回值缺少长度注解触发 `Bail out!` 断言，
/// 以 SIGABRT 终止 gnome-shell，表现为"一点截图就被登出"。
/// gnome-shell 自身的 `org.gnome.Shell.Screenshot` D-Bus 接口自 GNOME 42 起对第三方客户端
/// 一律返回 `AccessDenied`，因此 XDG Desktop Portal 是 Wayland 下唯一受支持的截屏入口。
pub async fn acquire_screen_snapshot() -> Result<RgbaImage> {
    let t_start = Instant::now();
    let caps = get_platform_capabilities();

    tracing::debug!(
        "准备通过 XDG Desktop Portal 捕获全屏 (桌面: {}，Wayland: {})",
        caps.desktop_env,
        caps.is_wayland
    );

    let rgba = capture_fullscreen_portal()
        .await
        .context("Portal 全屏截图捕获失败")?;
    tracing::info!(
        "XDG Portal 全屏快照完成: {}x{}，耗时: {:.2}ms",
        rgba.width(),
        rgba.height(),
        Instant::now()
            .saturating_duration_since(t_start)
            .as_secs_f64()
            * 1000.0
    );
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
            SnipperOutcome::CopiedImage => {
                if cfg.output.notify_on_copy {
                    send_desktop_notification("Snapin 截图", "选区图像已成功复制到剪贴板").await?;
                }
            }
            SnipperOutcome::CopiedText(text) => {
                if cfg.output.notify_on_copy {
                    let summary =
                        format!("识别文字已复制到剪贴板（{} 字符）", text.chars().count());
                    send_desktop_notification("Snapin 文字识别", &summary).await?;
                }
            }
            SnipperOutcome::Saved(path) => {
                if cfg.output.notify_on_save {
                    let msg = format!("选区图像已保存至: {}", path.display());
                    send_desktop_notification("Snapin 截图已保存", &msg).await?;
                }
            }
            SnipperOutcome::Pinned(item) => {
                tracing::info!("正在向常驻集中贴图工作区投递选区窗口...");
                let raw_path = AppPaths::generate_temp_path("paster_ipc", "raw");
                let mut delivered = false;
                if snapin_core::AppPaths::write_secure_file(&raw_path, item.pixmap.data()).is_ok() {
                    let req = snapin_daemon::DaemonRequest::CreatePaster {
                        width: item.pixmap.width(),
                        height: item.pixmap.height(),
                        x: item.position.0,
                        y: item.position.1,
                        title: item.title.clone(),
                        raw_path: raw_path.to_string_lossy().to_string(),
                    };
                    if let Ok(true) = snapin_daemon::send_command_to_daemon(&req).await {
                        tracing::info!("已成功通过零序列化膨胀安全路径移交常驻集中贴图工作区展示");
                        delivered = true;
                    } else {
                        let _ = std::fs::remove_file(&raw_path);
                    }
                }

                if !delivered {
                    tracing::info!("未检测到常驻守护服务，就地启动集中贴图工作区");
                    tokio::task::spawn_blocking(move || {
                        if let Err(e) = snapin_ui::paster::spawn_paster_window(item) {
                            tracing::error!("贴图工作区运行异常: {e}");
                        }
                    })
                    .await
                    .context("调度本地贴图工作区失败")?;
                }
            }
            SnipperOutcome::Cancelled => {
                tracing::info!("用户已取消本次截图");
            }
        }
    }

    Ok(())
}
