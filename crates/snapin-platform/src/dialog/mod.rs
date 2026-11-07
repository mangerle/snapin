//! 跨平台统一文件保存与选择对话框门面。
//!
//! 在 Linux (GNOME / Wayland) 环境下统一委托 XDG Desktop Portal 实现，遵照沙箱安全契约；
//! 在 Windows / macOS 环境下统一通过原生系统对话框（rfd）实现；
//! 其他平台显式返回不支持，不向上层泄漏平台差异条件编译。

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native;

use crate::error::PlatformError;
use std::path::{Path, PathBuf};

/// 打开系统原生保存对话框，供用户选择图像另存为目标路径。
///
/// # Errors
///
/// 对话框调用超时、Portal 拒绝或底层系统异常时返回 [`PlatformError`]。
pub async fn pick_save_path(
    suggested_name: &str,
    current_folder: Option<&Path>,
) -> Result<Option<PathBuf>, PlatformError> {
    #[cfg(target_os = "linux")]
    {
        crate::portal::pick_save_path(suggested_name, current_folder).await
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        native::pick_save_path(suggested_name, current_folder).await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = (suggested_name, current_folder);
        Err(PlatformError::Unsupported {
            capability: "系统保存对话框".to_string(),
        })
    }
}

/// 打开配置文件导入对话框。
///
/// # Errors
///
/// 对话框调用超时、Portal 拒绝或底层系统异常时返回 [`PlatformError`]。
pub async fn pick_config_import_path() -> Result<Option<PathBuf>, PlatformError> {
    #[cfg(target_os = "linux")]
    {
        crate::portal::pick_config_import_path().await
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        native::pick_config_import_path().await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(PlatformError::Unsupported {
            capability: "系统文件选择对话框".to_string(),
        })
    }
}

/// 打开配置文件导出对话框。
///
/// # Errors
///
/// 对话框调用超时、Portal 拒绝或底层系统异常时返回 [`PlatformError`]。
pub async fn pick_config_export_path() -> Result<Option<PathBuf>, PlatformError> {
    #[cfg(target_os = "linux")]
    {
        crate::portal::pick_config_export_path().await
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        native::pick_config_export_path().await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(PlatformError::Unsupported {
            capability: "系统文件保存对话框".to_string(),
        })
    }
}
