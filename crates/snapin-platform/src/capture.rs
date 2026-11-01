//! 统一跨平台全屏抓屏调度驱动。
//!
//! 在 Linux 平台优先走 GNOME Mutter 内部扩展以获得零白闪秒级原生截屏能力，
//! 扩展不在线时降级至 XDG Desktop Portal；
//! 在 Windows 与 macOS 平台直接调用 xcap 享受 DXGI 显存复制与 ScreenCaptureKit 毫秒级极速捕获。

use crate::error::PlatformError;
use image::RgbaImage;

#[cfg(target_os = "linux")]
use crate::{
    capabilities::get_platform_capabilities, extension_client::capture_fullscreen_extension,
    portal::capture_fullscreen_portal,
};
#[cfg(target_os = "linux")]
use snapin_core::AppPaths;

/// 跨平台获取全屏底图快照
///
/// 针对不同操作系统自动适配最优原生通路：
/// - Linux: GNOME 扩展零白闪优先，XDG Portal 兜底；
/// - Windows / macOS: xcap 原生极速通道。
pub async fn capture_fullscreen_snapshot() -> Result<RgbaImage, PlatformError> {
    #[cfg(target_os = "linux")]
    {
        capture_linux().await
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        capture_xcap().await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(PlatformError::Unsupported {
            capability: "屏幕快照捕获".to_string(),
        })
    }
}

/// Linux 平台专用抓屏逻辑：扩展直通与 Portal 降级
#[cfg(target_os = "linux")]
async fn capture_linux() -> Result<RgbaImage, PlatformError> {
    let caps = get_platform_capabilities();

    if caps.gnome_extension_active {
        match capture_via_extension().await {
            Ok(image) => return Ok(image),
            Err(err) => {
                tracing::warn!("GNOME 扩展零白闪通道未命中，降级至 XDG Portal 通道，原因: {err:#}");
            }
        }
    } else {
        tracing::debug!(
            "GNOME 扩展未在线（桌面: {}，Wayland: {}），启用 XDG Portal 抓屏",
            caps.desktop_env,
            caps.is_wayland
        );
    }

    capture_fullscreen_portal().await
}

/// 通过 GNOME Shell 扩展执行零白闪截屏并解码
#[cfg(target_os = "linux")]
async fn capture_via_extension() -> Result<RgbaImage, PlatformError> {
    let target_path = AppPaths::generate_temp_path("screen", "png");
    let target_str = target_path.to_string_lossy().to_string();

    let success = capture_fullscreen_extension(&target_str).await?;
    if !success {
        return Err(PlatformError::Capture {
            message: format!("GNOME 扩展返回截屏失败，目标路径: {target_str}"),
        });
    }

    let file_bytes = tokio::fs::read(&target_path)
        .await
        .map_err(|source| PlatformError::Io {
            path: target_str.clone(),
            source,
        })?;

    // 读取完成后立即清理临时文件
    let _ = tokio::fs::remove_file(&target_path).await;

    tokio::task::spawn_blocking(move || {
        image::load_from_memory(&file_bytes)
            .map(|dynamic_img| dynamic_img.to_rgba8())
            .map_err(|e| PlatformError::ImageDecode {
                message: format!("从扩展导出的 PNG 图像解码失败: {e}"),
            })
    })
    .await
    .map_err(|error| PlatformError::Capture {
        message: format!("扩展截图解码任务异常: {error}"),
    })?
}

/// Windows / macOS 平台专用抓屏逻辑：基于 xcap 抓取主屏幕或多屏合成
#[cfg(any(target_os = "windows", target_os = "macos"))]
async fn capture_xcap() -> Result<RgbaImage, PlatformError> {
    tokio::task::spawn_blocking(|| {
        let monitors = xcap::Monitor::all().map_err(|e| PlatformError::Capture {
            message: format!("获取物理显示器列表失败: {e}"),
        })?;

        if monitors.is_empty() {
            return Err(PlatformError::Capture {
                message: "未检测到任何可用的物理显示器".to_string(),
            });
        }

        // 单显示器场景：直接极速捕获并转换
        if monitors.len() == 1 {
            let captured = monitors[0].capture_image().map_err(|e| {
                #[cfg(target_os = "macos")]
                let hint =
                    "（请检查是否已在“系统设置 -> 隐私与安全性 -> 屏幕录制”中授予 Snapin 权限）";
                #[cfg(not(target_os = "macos"))]
                let hint = "";
                PlatformError::Capture {
                    message: format!("显示器捕获失败: {e}{hint}"),
                }
            })?;
            let width = captured.width();
            let height = captured.height();
            let raw_pixels = captured.into_raw();
            return RgbaImage::from_raw(width, height, raw_pixels).ok_or_else(|| {
                PlatformError::Capture {
                    message: "显示器捕获像素缓冲区转换为图像失败".to_string(),
                }
            });
        }

        // 多显示器场景：计算包围整个虚拟桌面的联合边界
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        let mut monitor_frames = Vec::with_capacity(monitors.len());
        for m in &monitors {
            let x = m.x().unwrap_or(0);
            let y = m.y().unwrap_or(0);
            let w = m.width().unwrap_or(0);
            let h = m.height().unwrap_or(0);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x + w as i32);
            max_y = max_y.max(y + h as i32);

            let frame = m.capture_image().map_err(|e| PlatformError::Capture {
                message: format!(
                    "多屏环境中捕获显示器 {} 失败: {e}",
                    m.name().unwrap_or_else(|_| "未知".to_string())
                ),
            })?;
            let fw = frame.width();
            let fh = frame.height();
            let raw = frame.into_raw();
            if let Some(rgba_frame) = RgbaImage::from_raw(fw, fh, raw) {
                monitor_frames.push((x, y, rgba_frame));
            }
        }

        let total_w = (max_x - min_x).max(1) as u32;
        let total_h = (max_y - min_y).max(1) as u32;
        let mut combined = RgbaImage::new(total_w, total_h);

        // 将每个显示器捕获的帧写入合成虚拟桌面
        for (x, y, frame) in monitor_frames {
            let offset_x = (x - min_x) as u32;
            let offset_y = (y - min_y) as u32;
            image::imageops::overlay(&mut combined, &frame, offset_x.into(), offset_y.into());
        }

        Ok(combined)
    })
    .await
    .map_err(|e| PlatformError::Capture {
        message: format!("异步调度抓屏任务失败: {e}"),
    })?
}
