//! 统一跨平台全屏抓屏调度驱动。
//!
//! 在 Linux 平台优先走 GNOME Mutter 内部扩展以获得零白闪秒级原生截屏能力，
//! 扩展不在线时降级至 XDG Desktop Portal；
//! 在 Windows 与 macOS 平台直接调用 xcap 享受 DXGI 显存复制与 ScreenCaptureKit 毫秒级极速捕获。

use crate::error::PlatformError;
use image::RgbaImage;
use std::time::Duration;

/// 原生抓屏或解码任务的最长等待时间，避免 macOS 权限弹窗或驱动挂起拖死整条流程。
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
/// 单张全屏 PNG 文件的最大字节数，防止异常文件一次性读入造成内存压力。
#[cfg(target_os = "linux")]
const MAX_SCREENSHOT_FILE_BYTES: u64 = 256 * 1024 * 1024;

#[cfg(target_os = "linux")]
use crate::{capabilities::get_platform_capabilities, portal::capture_fullscreen_portal};
#[cfg(target_os = "linux")]
use snapin_foundation::AppPaths;

/// 跨平台获取全屏底图快照
///
/// 针对不同操作系统自动适配最优原生通路：
/// - Linux: GNOME 扩展零白闪优先，XDG Portal 兜底；
/// - Windows / macOS: xcap 原生极速通道。
pub async fn capture_fullscreen_snapshot(include_cursor: bool) -> Result<RgbaImage, PlatformError> {
    #[cfg(target_os = "linux")]
    {
        capture_linux(include_cursor).await
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        if include_cursor {
            tracing::warn!("当前平台的 xcap 截屏通道不包含鼠标指针，本次将忽略该选项");
        }
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
async fn capture_linux(include_cursor: bool) -> Result<RgbaImage, PlatformError> {
    let caps = get_platform_capabilities();

    if caps.gnome_extension_active {
        match capture_via_extension(include_cursor).await {
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

    if include_cursor {
        tracing::warn!("XDG Portal 截屏通道不包含鼠标指针，本次将回退为无指针截图");
    }
    capture_fullscreen_portal().await
}

/// 通过 GNOME Shell 扩展执行零白闪截屏并解码
#[cfg(target_os = "linux")]
async fn capture_via_extension(include_cursor: bool) -> Result<RgbaImage, PlatformError> {
    let target_path = AppPaths::generate_temp_path("screen", "png");
    let target_str = target_path.to_string_lossy().to_string();
    let success = crate::extension_client::capture_fullscreen_extension_with_cursor(
        &target_str,
        include_cursor,
    )
    .await?;
    if !success {
        return Err(PlatformError::Capture {
            message: format!("GNOME 扩展返回截屏失败，目标路径: {target_str}"),
        });
    }
    let file_bytes = read_extension_screenshot(&target_path, &target_str).await?;
    decode_extension_screenshot(file_bytes).await
}

/// 读取扩展输出文件，限制大小并立即回收临时文件。
async fn read_extension_screenshot(
    target_path: &std::path::Path,
    target_str: &str,
) -> Result<Vec<u8>, PlatformError> {
    let metadata = tokio::fs::metadata(target_path)
        .await
        .map_err(|source| PlatformError::Io {
            path: target_str.to_string(),
            source,
        })?;
    if metadata.len() > MAX_SCREENSHOT_FILE_BYTES {
        let _ = tokio::fs::remove_file(target_path).await;
        return Err(PlatformError::Capture {
            message: format!(
                "GNOME 扩展导出的截图文件过大，大小: {} 字节，上限: {MAX_SCREENSHOT_FILE_BYTES} 字节",
                metadata.len()
            ),
        });
    }
    let bytes = tokio::fs::read(target_path)
        .await
        .map_err(|source| PlatformError::Io {
            path: target_str.to_string(),
            source,
        })?;
    let _ = tokio::fs::remove_file(target_path).await;
    Ok(bytes)
}

/// 在阻塞池解码扩展导出的 PNG，避免阻塞异步 worker。
async fn decode_extension_screenshot(file_bytes: Vec<u8>) -> Result<RgbaImage, PlatformError> {
    tokio::time::timeout(
        CAPTURE_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            image::load_from_memory(&file_bytes)
                .map(|dynamic_img| dynamic_img.to_rgba8())
                .map_err(|error| PlatformError::ImageDecode {
                    message: format!("从扩展导出的 PNG 图像解码失败: {error}"),
                })
        }),
    )
    .await
    .map_err(|_| PlatformError::Capture {
        message: format!("扩展截图解码超时（{} 秒）", CAPTURE_TIMEOUT.as_secs()),
    })?
    .map_err(|error| PlatformError::Capture {
        message: format!("扩展截图解码任务异常: {error}"),
    })?
}

/// Windows / macOS 平台专用抓屏逻辑：基于 xcap 抓取主屏幕或多屏合成
#[cfg(any(target_os = "windows", target_os = "macos"))]
async fn capture_xcap() -> Result<RgbaImage, PlatformError> {
    tokio::time::timeout(
        CAPTURE_TIMEOUT,
        tokio::task::spawn_blocking(capture_xcap_blocking),
    )
    .await
    .map_err(|_| PlatformError::Capture {
        message: format!("屏幕捕获任务超时（{} 秒）", CAPTURE_TIMEOUT.as_secs()),
    })?
    .map_err(|error| PlatformError::Capture {
        message: format!("异步调度抓屏任务失败: {error}"),
    })?
}

/// 在阻塞线程中枚举显示器并选择单屏直读或多屏合成。
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_xcap_blocking() -> Result<RgbaImage, PlatformError> {
    let monitors = xcap::Monitor::all().map_err(|error| PlatformError::Capture {
        message: format!("获取物理显示器列表失败: {error}"),
    })?;
    if monitors.is_empty() {
        return Err(PlatformError::Capture {
            message: "未检测到任何可用的物理显示器".to_string(),
        });
    }
    if monitors.len() == 1 {
        return capture_single_monitor(&monitors[0]);
    }
    capture_monitor_desktop(&monitors)
}

/// 捕获单显示器并转换像素布局。
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_single_monitor(monitor: &xcap::Monitor) -> Result<RgbaImage, PlatformError> {
    let captured = monitor.capture_image().map_err(|error| {
        #[cfg(target_os = "macos")]
        let hint = "（请检查是否已在“系统设置 -> 隐私与安全性 -> 屏幕录制”中授予 Snapin 权限）";
        #[cfg(not(target_os = "macos"))]
        let hint = "";
        PlatformError::Capture {
            message: format!("显示器捕获失败: {error}{hint}"),
        }
    })?;
    let (width, height) = (captured.width(), captured.height());
    RgbaImage::from_raw(width, height, captured.into_raw()).ok_or_else(|| PlatformError::Capture {
        message: "显示器捕获像素缓冲区转换为图像失败".to_string(),
    })
}

/// 捕获全部显示器并合成到虚拟桌面坐标系。
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_monitor_desktop(monitors: &[xcap::Monitor]) -> Result<RgbaImage, PlatformError> {
    let mut bounds = DesktopBounds::default();
    let mut frames = Vec::with_capacity(monitors.len());
    for monitor in monitors {
        let origin = (
            monitor.x().unwrap_or(0),
            monitor.y().unwrap_or(0),
            monitor.width().unwrap_or(0),
            monitor.height().unwrap_or(0),
        );
        bounds.include(origin);
        let frame = monitor
            .capture_image()
            .map_err(|error| PlatformError::Capture {
                message: format!(
                    "多屏环境中捕获显示器 {} 失败: {error}",
                    monitor.name().unwrap_or_else(|_| "未知".to_string())
                ),
            })?;
        if let Some(frame) = RgbaImage::from_raw(frame.width(), frame.height(), frame.into_raw()) {
            frames.push((origin.0, origin.1, frame));
        }
    }
    Ok(bounds.compose(frames))
}

/// 多显示器虚拟桌面联合边界。
#[derive(Default)]
#[cfg(any(target_os = "windows", target_os = "macos"))]
struct DesktopBounds {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    initialized: bool,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl DesktopBounds {
    fn include(&mut self, (x, y, width, height): (i32, i32, u32, u32)) {
        if !self.initialized {
            (self.min_x, self.min_y, self.max_x, self.max_y) = (x, y, x, y);
            self.initialized = true;
        }
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x.saturating_add(width as i32));
        self.max_y = self.max_y.max(y.saturating_add(height as i32));
    }

    fn compose(&self, frames: Vec<(i32, i32, RgbaImage)>) -> RgbaImage {
        let width = self.max_x.saturating_sub(self.min_x).max(1) as u32;
        let height = self.max_y.saturating_sub(self.min_y).max(1) as u32;
        let mut combined = RgbaImage::new(width, height);
        for (x, y, frame) in frames {
            image::imageops::overlay(
                &mut combined,
                &frame,
                (x - self.min_x).into(),
                (y - self.min_y).into(),
            );
        }
        combined
    }
}
