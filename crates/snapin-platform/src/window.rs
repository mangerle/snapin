//! 跨平台可见窗口几何边界枚举模块。
//!
//! 提供统一的可见窗口矩形查询接口，用于截图界面的鼠标悬停智能吸附。
//! 在 Linux 平台优先通过 GNOME 扩展读取窗口栈，
//! 在 Windows 与 macOS 平台直接基于底层系统窗口树接口枚举。

use crate::error::PlatformError;

/// 可见窗口几何边界矩形（物理像素坐标与尺寸）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[cfg(target_os = "linux")]
use crate::capabilities::get_platform_capabilities;
#[cfg(target_os = "linux")]
use crate::extension_client::list_windows as list_windows_gnome;

/// 枚举当前工作区中所有可见窗口的物理像素几何边界。
///
/// 顺序为自底向上或平台默认栈序，调用方应反向查找以优先吸附最上层窗口。
pub async fn enumerate_visible_windows() -> Result<Vec<WindowRect>, PlatformError> {
    #[cfg(target_os = "linux")]
    {
        enumerate_linux().await
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        enumerate_xcap().await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Ok(Vec::new())
    }
}

/// Linux 平台窗口枚举：依赖 GNOME 扩展 Mutter 特权通道
#[cfg(target_os = "linux")]
async fn enumerate_linux() -> Result<Vec<WindowRect>, PlatformError> {
    if get_platform_capabilities().gnome_extension_active {
        list_windows_gnome().await
    } else {
        // 非 GNOME 或扩展未在线时优雅降级为空列表，不阻断正常框选截图流程
        Ok(Vec::new())
    }
}

/// Windows / macOS 平台窗口枚举：通过 xcap 读取系统窗口拓扑
#[cfg(any(target_os = "windows", target_os = "macos"))]
async fn enumerate_xcap() -> Result<Vec<WindowRect>, PlatformError> {
    tokio::task::spawn_blocking(|| {
        let windows = xcap::Window::all().map_err(|e| PlatformError::Capture {
            message: format!("枚举系统窗口拓扑失败: {e}"),
        })?;

        let mut rects = Vec::with_capacity(windows.len());
        for win in windows {
            // 过滤最小化、零面积以及不可见窗口
            let is_minimized = win.is_minimized().unwrap_or(false);
            let width = win.width().unwrap_or(0);
            let height = win.height().unwrap_or(0);
            if is_minimized || width == 0 || height == 0 {
                continue;
            }

            // 过滤 snapin 自身界面窗口
            let app_name = win.app_name().unwrap_or_default().to_lowercase();
            let title = win.title().unwrap_or_default().to_lowercase();
            if app_name.contains("snapin") || title.contains("snapin") {
                continue;
            }

            let x = win.x().unwrap_or(0);
            let y = win.y().unwrap_or(0);

            rects.push(WindowRect {
                x,
                y,
                width,
                height,
            });
        }

        Ok(rects)
    })
    .await
    .map_err(|e| PlatformError::Capture {
        message: format!("异步执行窗口枚举失败: {e}"),
    })?
}
