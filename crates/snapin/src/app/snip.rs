//! 截图流水线调度与呈现模块。

use anyhow::{Context, Result};
use image::RgbaImage;
use snapin_core::config::Config;
use snapin_core::AppPaths;
use snapin_platform::{
    capture_fullscreen_extension, capture_fullscreen_portal, get_platform_capabilities,
    send_desktop_notification,
};
use snapin_ui::snipper::{run_snipper_session, SnipMode, SnipperOutcome};
use std::time::{Duration, Instant};

/// 获取屏幕快照（优先扩展零白闪通道，未命中则优雅降级 XDG Desktop Portal）。
///
/// 通道沿革：曾存在一条更快的"在扩展进程内 read_pixels 直取原始像素"通道，但其数组返回值
/// 在 GNOME 50 的 GJS 上命中 gi/arg.cpp 编解码断言，直接以 SIGABRT 终止 gnome-shell，
/// 表现为按一次截图就被登出。本机历史上该通道成功 0 次、而下面这条 PNG 通道成功 46 次，
/// 故原始像素通道已永久移除，只保留像素不跨 GJS 边界传递的 PNG 通道。
pub async fn acquire_screen_snapshot() -> Result<RgbaImage> {
    let t_start = Instant::now();
    let caps = get_platform_capabilities();

    if caps.gnome_extension_active {
        match capture_via_extension(t_start).await {
            Ok(image) => return Ok(image),
            Err(error) => {
                tracing::warn!(
                    "GNOME 扩展零白闪截屏通道未命中，降级 XDG Portal（该通道带快门闪屏）。\
                     若刚升级过软件包，扩展 JS 无法热更新，需注销重登一次方可生效；原因: {error:#}"
                )
            }
        }
    } else {
        tracing::debug!(
            "GNOME 扩展未在线（桌面: {}，Wayland: {}），直接走 XDG Portal 通道",
            caps.desktop_env,
            caps.is_wayland
        );
    }

    let rgba = capture_fullscreen_portal()
        .await
        .context("Portal 全屏截图捕获失败")?;
    Ok(rgba)
}

/// 经配套扩展的原生 `Shell.Screenshot` 通道完成一次全屏 PNG 捕获与解码
async fn capture_via_extension(t_start: Instant) -> Result<RgbaImage> {
    let target_path = AppPaths::generate_temp_path("screen", "png");
    let target_str = target_path.to_string_lossy().to_string();

    if !capture_fullscreen_extension(&target_str)
        .await
        .context("请求 GNOME 扩展执行原生截屏失败")?
    {
        anyhow::bail!("GNOME 扩展返回截屏失败，目标: {target_str}");
    }

    let bytes = tokio::fs::read(&target_path)
        .await
        .context("读取扩展输出的全屏快照失败")?;
    let _ = tokio::fs::remove_file(&target_path).await;

    let rgba = image::load_from_memory(&bytes)
        .context("全屏快照 PNG 解码失败")?
        .into_rgba8();

    tracing::info!(
        "命中 GNOME 扩展原生零白闪截屏通道，全屏截屏总耗时: {:.2}ms",
        Instant::now()
            .saturating_duration_since(t_start)
            .as_secs_f64()
            * 1000.0
    );
    Ok(rgba)
}

/// 执行单次截图流程
///
/// `delay_seconds` 大于 0 时，先发送桌面通知并等待指定秒数再捕获屏幕，
/// 便于截取菜单、悬浮提示等需要先操作的界面。
pub async fn run_oneshot_snip(cfg: Config, mode: SnipMode, delay_seconds: u32) -> Result<()> {
    if delay_seconds > 0 {
        let message = format!("将在 {delay_seconds} 秒后截图");
        if let Err(error) = send_desktop_notification("Snapin 截图", &message).await {
            tracing::debug!("发送延时截图通知失败或系统通知不可用: {error}");
        }
        tracing::info!("截图延时等待中: {delay_seconds} 秒");
        tokio::time::sleep(Duration::from_secs(u64::from(delay_seconds))).await;
    }

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
            SnipperOutcome::CopiedColor(color) => {
                if cfg.output.notify_on_copy {
                    let summary = format!("颜色值已复制到剪贴板: {color}");
                    send_desktop_notification("Snapin 取色器", &summary).await?;
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
