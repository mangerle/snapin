//! 截图流水线调度与呈现模块。

use anyhow::{Context, Result};
use image::RgbaImage;
use snapin_core::config::Config;
use snapin_platform::{
    capture_fullscreen_extension, capture_fullscreen_extension_raw, capture_fullscreen_portal,
    generate_secure_temp_path, get_platform_capabilities, send_desktop_notification,
};
use snapin_ui::snipper::{run_snipper_session, SnipMode, SnipperOutcome};
use std::time::Instant;

/// 获取屏幕快照（依据全局平台能力缓存选择最优通道，彻底消除无用 D-Bus 探测等待）
pub async fn acquire_screen_snapshot() -> Result<RgbaImage> {
    let t_start = Instant::now();
    let caps = get_platform_capabilities();

    // 1. 若全局能力缓存确认 GNOME 扩展在线，优先尝试扩展极速通道
    if caps.gnome_extension_active {
        // 1.1 尝试零延迟 Raw 原始物理像素截屏（跳过压缩与解码）
        let raw_path = generate_secure_temp_path("screen", "raw");
        let raw_str = raw_path.to_string_lossy().to_string();
        if let Ok(Some((w, h))) = capture_fullscreen_extension_raw(&raw_str).await {
            if let Ok(bytes) = std::fs::read(&raw_path) {
                let _ = std::fs::remove_file(&raw_path);
                if bytes.len() == (w as usize) * (h as usize) * 4 {
                    if let Some(rgba) = RgbaImage::from_raw(w, h, bytes) {
                        tracing::info!(
                            "命中 GNOME 扩展原生零延迟 Raw 像素通道，全屏截屏总耗时: {:.2}ms",
                            t_start.elapsed().as_secs_f64() * 1000.0
                        );
                        return Ok(rgba);
                    }
                }
            }
        }

        // 1.2 降级尝试 GNOME 扩展 PNG 截屏通道
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
    } else {
        tracing::debug!(
            "GNOME 扩展未在线（环境: {}，Wayland: {}），直接走 XDG Portal 通道，消除 D-Bus 试探延迟",
            caps.desktop_env,
            caps.is_wayland
        );
    }

    // 2. 扩展未激活或调用失败时，走 XDG Desktop Portal 通道
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
                tracing::info!("正在向常驻集中贴图工作区投递选区窗口...");
                let raw_path = generate_secure_temp_path("paster_ipc", "raw");
                let mut delivered = false;
                if std::fs::write(&raw_path, item.pixmap.data()).is_ok() {
                    let req = snapin_daemon::DaemonCommand::CreatePaster {
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
