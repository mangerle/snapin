//! 统一跨平台全屏抓屏调度驱动。
//!
//! 在 Linux 平台优先走 GNOME Mutter 内部扩展以获得零白闪秒级原生截屏能力，
//! 扩展不在线时降级至 XDG Desktop Portal；
//! 在 Windows 与 macOS 平台直接调用 xcap 享受 DXGI 显存复制与 ScreenCaptureKit 毫秒级极速捕获。

use crate::error::PlatformError;
use image::RgbaImage;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::time::Duration;

/// 原生抓屏或解码任务的最长等待时间，避免 macOS 权限弹窗或驱动挂起拖死整条流程。
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
/// 跨平台屏幕捕获允许合成的最大像素数，与原生像素载荷上限保持一致。
#[cfg(any(target_os = "windows", target_os = "macos", test))]
const MAX_CAPTURE_PIXELS: u64 = 100_000_000;
/// 单张全屏 PNG 文件的最大字节数，防止异常文件一次性读入造成内存压力。
#[cfg(target_os = "linux")]
pub(crate) const MAX_SCREENSHOT_FILE_BYTES: u64 = 256 * 1024 * 1024;

#[cfg(target_os = "linux")]
use crate::{capabilities::get_platform_capabilities, portal::capture_fullscreen_portal};
#[cfg(target_os = "linux")]
use snapin_foundation::AppPaths;

/// 跨平台获取全屏底图快照
///
/// 针对不同操作系统自动适配最优原生通路：
/// - Linux: GNOME 扩展零白闪优先，XDG Portal 兜底；
/// - Windows / macOS: xcap 原生极速通道。
///
/// # Errors
///
/// 运行时目录、屏幕捕获、图像解码、尺寸校验或超时失败时返回 [`PlatformError`]。
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
    let target_path =
        AppPaths::generate_temp_path("screen", "png").map_err(|source| PlatformError::Io {
            path: AppPaths::runtime_dir().display().to_string(),
            source,
        })?;
    let _cleanup = TemporaryScreenshotFile::new(target_path.clone());
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
    let file_bytes = read_extension_screenshot(target_path, target_str).await?;
    decode_extension_screenshot(file_bytes).await
}

/// 读取扩展输出文件，并在读取阶段强制施加大小上限。
#[cfg(target_os = "linux")]
async fn read_extension_screenshot(
    target_path: PathBuf,
    target_str: String,
) -> Result<Vec<u8>, PlatformError> {
    tokio::time::timeout(
        CAPTURE_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            snapin_foundation::read_file_limited(&target_path, MAX_SCREENSHOT_FILE_BYTES as usize)
                .map_err(|error| PlatformError::Capture {
                    message: format!("读取 GNOME 扩展截图失败，路径: {target_str}，原因: {error}"),
                })
        }),
    )
    .await
    .map_err(|_| PlatformError::Capture {
        message: format!(
            "读取 GNOME 扩展截图超时（{} 秒）",
            CAPTURE_TIMEOUT.as_secs()
        ),
    })?
    .map_err(|error| PlatformError::Capture {
        message: format!("等待 GNOME 扩展截图读取任务失败: {error}"),
    })?
}

/// 在阻塞池解码扩展导出的 PNG，避免阻塞异步 worker。
#[cfg(target_os = "linux")]
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
        let (x, y, width, height) = capture_monitor_geometry(monitor)?;
        bounds.include(x, y, width, height)?;
        let monitor_name = monitor.name().unwrap_or_else(|_| "未知显示器".to_string());
        let frame = monitor
            .capture_image()
            .map_err(|error| PlatformError::Capture {
                message: format!("多屏环境中捕获显示器 {monitor_name} 失败: {error}"),
            })?;
        let frame = RgbaImage::from_raw(frame.width(), frame.height(), frame.into_raw())
            .ok_or_else(|| PlatformError::Capture {
                message: format!("显示器 {monitor_name} 的像素缓冲区转换为图像失败"),
            })?;
        frames.push((x, y, frame));
    }
    bounds.compose(frames)
}

/// 读取单个显示器坐标与尺寸；任一项失败都返回明确错误而不是伪造为零。
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_monitor_geometry(
    monitor: &xcap::Monitor,
) -> Result<(i32, i32, u32, u32), PlatformError> {
    let name = monitor.name().unwrap_or_else(|_| "未知显示器".to_string());
    let x = monitor.x().map_err(|error| PlatformError::Capture {
        message: format!("读取显示器 {name} 的 X 坐标失败: {error}"),
    })?;
    let y = monitor.y().map_err(|error| PlatformError::Capture {
        message: format!("读取显示器 {name} 的 Y 坐标失败: {error}"),
    })?;
    let width = monitor.width().map_err(|error| PlatformError::Capture {
        message: format!("读取显示器 {name} 的宽度失败: {error}"),
    })?;
    let height = monitor.height().map_err(|error| PlatformError::Capture {
        message: format!("读取显示器 {name} 的高度失败: {error}"),
    })?;
    Ok((x, y, width, height))
}

/// 临时截图文件的取消安全清理守卫。
///
/// 即使截图调用被取消或后续解码失败，析构也会同步删除文件，避免长期运行目录
/// 因异常路径持续积累全屏截图。
#[cfg(target_os = "linux")]
pub(crate) struct TemporaryScreenshotFile {
    path: PathBuf,
}

#[cfg(target_os = "linux")]
impl TemporaryScreenshotFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[cfg(target_os = "linux")]
impl Drop for TemporaryScreenshotFile {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("清理临时截图文件失败，路径: {:?}，原因: {error}", self.path);
            }
        }
    }
}

/// 多显示器虚拟桌面联合边界。
#[derive(Default)]
#[cfg(any(target_os = "windows", target_os = "macos", test))]
struct DesktopBounds {
    min_x: i64,
    min_y: i64,
    max_x: i64,
    max_y: i64,
    initialized: bool,
}

#[cfg(any(target_os = "windows", target_os = "macos", test))]
impl DesktopBounds {
    fn include(&mut self, x: i32, y: i32, width: u32, height: u32) -> Result<(), PlatformError> {
        if width == 0 || height == 0 {
            return Err(PlatformError::Capture {
                message: format!("显示器尺寸非法，坐标: ({x}, {y})，尺寸: {width}x{height}"),
            });
        }
        let x = i64::from(x);
        let y = i64::from(y);
        let right = x + i64::from(width);
        let bottom = y + i64::from(height);
        if !self.initialized {
            (self.min_x, self.min_y, self.max_x, self.max_y) = (x, y, right, bottom);
            self.initialized = true;
        }
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(right);
        self.max_y = self.max_y.max(bottom);
        Ok(())
    }

    fn compose(&self, frames: Vec<(i32, i32, RgbaImage)>) -> Result<RgbaImage, PlatformError> {
        if !self.initialized || frames.is_empty() {
            return Err(PlatformError::Capture {
                message: "多屏捕获未产生任何有效显示器帧".to_string(),
            });
        }
        let width = u32::try_from(self.max_x - self.min_x).map_err(|_| PlatformError::Capture {
            message: "多屏虚拟桌面宽度超出支持范围".to_string(),
        })?;
        let height =
            u32::try_from(self.max_y - self.min_y).map_err(|_| PlatformError::Capture {
                message: "多屏虚拟桌面高度超出支持范围".to_string(),
            })?;
        let pixels = u64::from(width).saturating_mul(u64::from(height));
        if pixels == 0 || pixels > MAX_CAPTURE_PIXELS {
            return Err(PlatformError::Capture {
                message: format!(
                    "多屏虚拟桌面像素数非法，尺寸: {width}x{height}，上限: {MAX_CAPTURE_PIXELS}"
                ),
            });
        }
        let mut combined = RgbaImage::new(width, height);
        for (x, y, frame) in frames {
            image::imageops::overlay(
                &mut combined,
                &frame,
                i64::from(x) - self.min_x,
                i64::from(y) - self.min_y,
            );
        }
        Ok(combined)
    }
}

#[cfg(test)]
mod bounds_tests {
    use super::*;

    #[test]
    fn test_desktop_bounds_rejects_empty_or_zero_sized_frames() {
        let mut bounds = DesktopBounds::default();
        assert!(bounds.compose(Vec::new()).is_err());
        assert!(bounds.include(0, 0, 0, 100).is_err());

        bounds.include(-100, -50, 200, 100).expect("合法边界应接受");
        let frame = RgbaImage::new(1, 1);
        let combined = bounds
            .compose(vec![(-100, -50, frame)])
            .expect("有效帧应可合成");
        assert_eq!(combined.dimensions(), (200, 100));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn test_temporary_screenshot_file_is_removed_on_drop() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("screen.png");
        std::fs::write(&path, b"screenshot").expect("应可写入临时截图");
        let guard = TemporaryScreenshotFile::new(path.clone());

        drop(guard);

        assert!(!path.exists(), "守卫析构后临时截图必须被删除");
    }
}
