//! 基于 XDG Desktop Portal 的 Wayland 屏幕图像捕获模块。
//!
//! 在 Wayland / Ubuntu 26 GNOME 环境下，遵照 Freedesktop 安全沙箱规范，
//! 请求系统特权合成器交付完整屏幕帧缓冲区。

use crate::error::PlatformError;
use ashpd::desktop::file_chooser::{FileFilter, SelectedFiles};
use ashpd::desktop::screenshot::Screenshot;
use ashpd::desktop::ResponseError;
use image::RgbaImage;
use std::fs;
use std::path::{Path, PathBuf};

/// 请求 XDG Desktop Portal 执行静默全屏截屏（非交互模式）。
///
/// 成功获取后立即将图片载入内存，并安全清理本地临时文件。
pub async fn capture_fullscreen_portal() -> Result<RgbaImage, PlatformError> {
    tracing::debug!("正在向 XDG Desktop Portal 请求全屏快照...");

    let t_req_start = std::time::Instant::now();
    let portal_fut = async {
        Screenshot::request()
            .interactive(false)
            .modal(false)
            .send()
            .await
            .map_err(|e| PlatformError::Portal {
                message: e.to_string(),
            })?
            .response()
            .map_err(|e| PlatformError::Portal {
                message: e.to_string(),
            })
    };

    let response = tokio::time::timeout(std::time::Duration::from_secs(5), portal_fut)
        .await
        .map_err(|_| PlatformError::Portal {
            message: "等待 XDG Desktop Portal 响应超时".to_string(),
        })??;
    let t_req_end = std::time::Instant::now();
    tracing::info!(
        "Portal D-Bus 截屏请求与合成器捕获耗时: {:.2}ms",
        (t_req_end - t_req_start).as_secs_f64() * 1000.0
    );

    let uri = response.uri();
    let file_path = uri.to_file_path().map_err(|_| PlatformError::Portal {
        message: format!("无法将 Portal 返回的 URI 转换为文件路径: {uri:?}"),
    })?;

    tracing::debug!("Portal 截屏文件就绪，路径: {:?}", file_path);

    let t_dec_start = std::time::Instant::now();
    let bytes = fs::read(&file_path).map_err(|e| PlatformError::Io {
        path: file_path.to_string_lossy().to_string(),
        source: e,
    })?;
    if let Err(e) = fs::remove_file(&file_path) {
        tracing::warn!("清理 Portal 临时图片文件失败: {e}");
    }
    let dynamic_img = image::load_from_memory(&bytes).map_err(|e| PlatformError::ImageDecode {
        message: format!(
            "Portal 返回内容无法按受支持的图像格式解码，字节数: {}，原因: {e}",
            bytes.len()
        ),
    })?;
    let rgba_img = dynamic_img.into_rgba8();
    let t_dec_end = std::time::Instant::now();
    tracing::info!(
        "Portal 图片内存解码加载耗时: {:.2}ms",
        (t_dec_end - t_dec_start).as_secs_f64() * 1000.0
    );

    tracing::info!(
        "全屏图像加载完成，分辨率: {}x{}",
        rgba_img.width(),
        rgba_img.height()
    );
    Ok(rgba_img)
}

/// 打开 XDG Desktop Portal 保存对话框，让用户选择目标文件路径。
///
/// 返回 `Ok(None)` 表示用户主动取消；Wayland 下客户端无法自行弹出原生保存
/// 对话框，必须经由 Portal 获取用户授权后的路径。
///
/// # Errors
///
/// Portal 服务不可用、D-Bus 调用失败或返回的 URI 无法转换为本地路径时返回
/// [`PlatformError::Portal`]。
pub async fn pick_save_path(
    suggested_name: &str,
    current_folder: Option<&Path>,
) -> Result<Option<PathBuf>, PlatformError> {
    let request = SelectedFiles::save_file()
        .title("图像另存为")
        .accept_label("保存")
        .current_name(suggested_name)
        .modal(true)
        .filter(FileFilter::new("PNG 图像").glob("*.png"))
        .filter(FileFilter::new("JPEG 图像").glob("*.jpg").glob("*.jpeg"))
        .filter(FileFilter::new("WebP 图像").glob("*.webp"));

    let request = match current_folder {
        Some(folder) => request
            .current_folder::<PathBuf>(Some(folder.to_path_buf()))
            .map_err(|error| PlatformError::Portal {
                message: format!("设置保存对话框初始目录失败: {error}"),
            })?,
        None => request,
    };

    let response = request
        .send()
        .await
        .map_err(|error| PlatformError::Portal {
            message: format!("请求系统保存对话框失败: {error}"),
        })?
        .response();

    // Portal 将用户关闭对话框视为请求被取消而非故障，必须与真实错误区分：
    // 取消是正常用户操作，调用方据此保留当前界面而不提示保存失败。
    let files = match response {
        Ok(files) => files,
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => return Ok(None),
        Err(error) => {
            return Err(PlatformError::Portal {
                message: format!("等待保存对话框结果失败: {error}"),
            })
        }
    };

    let Some(uri) = files.uris().first() else {
        return Ok(None);
    };
    uri.to_file_path()
        .map(Some)
        .map_err(|_| PlatformError::Portal {
            message: format!("保存对话框返回的 URI 无法转换为本地路径: {uri}"),
        })
}
