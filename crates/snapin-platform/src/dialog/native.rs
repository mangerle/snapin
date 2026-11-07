//! 跨平台原生文件保存与选择对话框。

use crate::error::PlatformError;
use std::path::{Path, PathBuf};

/// 打开系统原生保存对话框，供用户选择图像另存为目标路径
#[cfg(not(target_os = "linux"))]
pub async fn pick_save_path(
    suggested_name: &str,
    current_folder: Option<&Path>,
) -> Result<Option<PathBuf>, PlatformError> {
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title("图像另存为")
        .set_file_name(suggested_name)
        .add_filter("PNG 图像", &["png"])
        .add_filter("JPEG 图像", &["jpg", "jpeg"])
        .add_filter("WebP 图像", &["webp"]);

    if let Some(folder) = current_folder {
        dialog = dialog.set_directory(folder);
    }

    let file_handle = dialog.save_file().await;
    Ok(file_handle.map(|handle| handle.path().to_path_buf()))
}

/// 打开配置文件导入对话框。
///
/// # Errors
///
/// 系统文件对话框不可用时返回 [`PlatformError::Dialog`]。
pub async fn pick_config_import_path() -> Result<Option<PathBuf>, PlatformError> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("导入 Snapin 配置")
        .add_filter("Snapin 配置", &["toml"])
        .pick_file()
        .await;
    Ok(file.map(|handle| handle.path().to_path_buf()))
}

/// 打开配置文件导出对话框。
///
/// # Errors
///
/// 系统文件对话框不可用时返回 [`PlatformError::Dialog`]。
pub async fn pick_config_export_path() -> Result<Option<PathBuf>, PlatformError> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("导出 Snapin 配置")
        .set_file_name("snapin-config.toml")
        .add_filter("Snapin 配置", &["toml"])
        .save_file()
        .await;
    Ok(file.map(|handle| handle.path().to_path_buf()))
}
