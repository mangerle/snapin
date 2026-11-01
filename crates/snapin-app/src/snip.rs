//! 截图流水线调度与呈现模块。

use crate::services::AppServices;
use anyhow::{Context, Result};
use image::RgbaImage;
use snapin_core::config::Config;
use snapin_core::AppPaths;
use snapin_platform::{
    capture_fullscreen_snapshot, enumerate_visible_windows, send_desktop_notification,
    WindowRect as PlatformWindowRect,
};
use snapin_ui::snipper::{
    run_snipper_session, SnipMode, SnipperOutcome, SnipperRuntime, WindowRect,
};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 获取全屏快照（跨平台调度：Linux 优先扩展零白闪/Portal降级，Windows/macOS 走原生极速捕获）。
pub async fn acquire_screen_snapshot() -> Result<RgbaImage> {
    let t_start = Instant::now();
    let rgba = capture_fullscreen_snapshot()
        .await
        .context("屏幕快照捕获失败")?;

    tracing::info!(
        "全屏快照捕获成功，图像尺寸: {}x{}，耗时: {:.2}ms",
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

    let services = Arc::new(AppServices::new(cfg.clone()).context("初始化截图应用服务失败")?);
    let windows = if matches!(
        mode,
        SnipMode::Normal | SnipMode::AutoCopy | SnipMode::Translate
    ) {
        match tokio::time::timeout(Duration::from_millis(500), enumerate_visible_windows()).await {
            Ok(Ok(windows)) => windows
                .into_iter()
                .map(to_ui_window_rect)
                .collect::<Vec<WindowRect>>(),
            Ok(Err(error)) => {
                tracing::debug!("获取窗口截图候选列表失败，继续使用普通区域截图: {error}");
                Vec::new()
            }
            Err(_) => {
                tracing::debug!("获取窗口截图候选列表超时，继续使用普通区域截图");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let runtime = SnipperRuntime {
        config: (*services.config()).clone(),
        mode,
        desktop: services.desktop(),
        media: services.media(),
    };
    let outcome = tokio::task::spawn_blocking(move || run_snipper_session(img, windows, runtime))
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
                let raw_written =
                    AppPaths::write_secure_file(&raw_path, item.pixmap.data()).is_ok();
                let mut delivered = false;

                if raw_written {
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
                    }
                }

                if !delivered {
                    if raw_written {
                        tracing::info!("未检测到常驻守护服务，改由独立进程启动集中贴图工作区");
                        if let Err(error) = spawn_detached_paster(&raw_path, item.position) {
                            let _ = std::fs::remove_file(&raw_path);
                            tracing::error!("启动独立贴图进程失败: {error:#}");
                            let _ = send_desktop_notification(
                                "Snapin 贴图失败",
                                "无法启动独立贴图进程，详情请查看运行日志",
                            )
                            .await;
                        }
                    } else {
                        tracing::error!("贴图像素临时文件写入失败，无法生成贴图");
                        let _ = send_desktop_notification(
                            "Snapin 贴图失败",
                            "贴图像素临时文件写入失败，请检查临时目录权限与磁盘空间",
                        )
                        .await;
                    }
                }
            }
            SnipperOutcome::Cancelled => {
                tracing::info!("用户已取消本次截图");
            }
        }
    }

    Ok(())
}

/// 将平台窗口矩形转换为 UI 几何契约，避免表现层依赖平台类型。
fn to_ui_window_rect(rect: PlatformWindowRect) -> WindowRect {
    WindowRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

/// 派生独立的 `snapin pin-file` 子进程完成一次性贴图。
///
/// winit 0.29 通过进程级静态标志限制 `EventLoop` 每进程只能创建一次，而截图会话已经
/// 占用了该名额；守护进程缺席时若仍在本进程创建贴图工作区，必然返回
/// `EventLoop can't be recreated`。因此必须把像素文件交给全新进程，由它复用
/// `pin-file` 既有的守护进程 IPC 与本地贴图回退逻辑，并在贴图关闭后自行退出。
/// 父进程不等待子进程，避免每个贴图都长期占用一个截图进程的内存。
fn spawn_detached_paster(raw_path: &Path, position: (i32, i32)) -> Result<()> {
    let exe = std::env::current_exe().context("获取当前可执行文件路径失败")?;
    Command::new(&exe)
        .arg("pin-file")
        .arg(raw_path)
        .arg(position.0.to_string())
        .arg(position.1.to_string())
        .spawn()
        .with_context(|| {
            format!(
                "启动独立贴图进程失败，可执行文件: {}，像素文件: {}",
                exe.display(),
                raw_path.display()
            )
        })?;
    Ok(())
}
