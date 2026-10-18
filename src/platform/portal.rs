//! 基于 XDG Desktop Portal 的 Wayland 屏幕图像捕获模块。
//!
//! 在 Wayland / Ubuntu 26 GNOME 环境下，遵照 Freedesktop 安全沙箱规范，
//! 请求系统特权合成器交付完整屏幕帧缓冲区。

use crate::error::SnapinError;
use ashpd::desktop::screenshot::Screenshot;
use image::DynamicImage;
use std::fs;

/// 请求 XDG Desktop Portal 执行静默全屏截屏（非交互模式）。
///
/// 成功获取后立即将图片载入内存，并安全清理本地临时文件。
pub async fn capture_fullscreen_portal() -> Result<DynamicImage, SnapinError> {
    tracing::debug!("正在向 XDG Desktop Portal 请求全屏快照...");

    let response = Screenshot::request()
        .interactive(false)
        .modal(false)
        .send()
        .await?
        .response()?;

    let uri = response.uri();
    let file_path = uri
        .to_file_path()
        .map_err(|_| SnapinError::SoftbufferError {
            message: format!("无法将 Portal 返回的 URI 转换为文件路径: {uri:?}"),
        })?;

    tracing::debug!("Portal 截屏文件就绪，路径: {:?}", file_path);

    let image = image::open(&file_path)?;

    // 内存加载完成后立即移除磁盘临时文件，防止垃圾堆积
    if let Err(e) = fs::remove_file(&file_path) {
        tracing::warn!("清理 Portal 临时图片文件失败: {e}");
    }

    tracing::info!(
        "全屏图像加载完成，分辨率: {}x{}",
        image.width(),
        image.height()
    );
    Ok(image)
}
