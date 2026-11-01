//! 基于 XDG Desktop Portal 的 Wayland 屏幕图像捕获模块。
//!
//! 在 Wayland / Ubuntu 26 GNOME 环境下，遵照 Freedesktop 安全沙箱规范，
//! 请求系统特权合成器交付完整屏幕帧缓冲区。

use crate::dbus_session::get_session_connection;
use crate::error::PlatformError;
use ashpd::desktop::file_chooser::{FileFilter, SelectedFiles};
use ashpd::desktop::screenshot::Screenshot;
use ashpd::desktop::ResponseError;
use image::RgbaImage;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 等待用户在保存对话框中作出选择的超时上限。
///
/// ashpd 0.8 的 `send()` 内部会等待 Portal 的 Response 信号，因此该超时覆盖
/// “建立请求 + 用户交互”的完整过程；没有上限时，用户打开对话框后离开会让
/// 保存任务永久挂起并拖住守护进程停机。
const PORTAL_DIALOG_TIMEOUT: Duration = Duration::from_secs(300);

/// 自愈确保当前应用在 XDG Desktop Portal 权限存储区具备合法的截屏授权。
///
/// 在 GNOME 50 Wayland 环境下，如果后台进程触发截屏且无前台焦点，
/// Portal 会因尝试弹窗而报错 `AccessDenied: Only the focused app is allowed to show a system access dialog`。
/// 通过在系统 PermissionStore 中预登记受信任授权，Portal 将直接交付帧缓冲而无需调起交互授权弹窗。
pub async fn ensure_portal_screenshot_permission() {
    let Ok(connection) = get_session_connection().await else {
        return;
    };

    // 针对 snapin 以及当前 app_id 写入已授权标记
    let app_ids = ["snapin", "snapin.desktop", ""];
    for app_id in app_ids {
        let _ = connection
            .call_method(
                Some("org.freedesktop.impl.portal.PermissionStore"),
                "/org/freedesktop/impl/portal/PermissionStore",
                Some("org.freedesktop.impl.portal.PermissionStore"),
                "SetPermission",
                &("screenshot", true, "screenshot", app_id, &["yes"]),
            )
            .await;
    }
}

/// 请求 XDG Desktop Portal 执行静默全屏截屏（非交互模式）。
///
/// 具备退避防抖重试机制（解决快捷键并发按压时 Portal 的 `ongoing operation` 冲突），
/// 并在调用前自动确保自愈权限，避免无焦点时抛出 `AccessDenied`。
pub async fn capture_fullscreen_portal() -> Result<RgbaImage, PlatformError> {
    tracing::debug!("正在向 XDG Desktop Portal 请求全屏快照...");

    // 预先确保后台静默截屏权限，防止被 GNOME 50 拦截
    ensure_portal_screenshot_permission().await;

    let t_req_start = std::time::Instant::now();
    let max_retries = 3;
    let mut last_err = None;

    let mut response = None;
    for attempt in 1..=max_retries {
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

        match tokio::time::timeout(Duration::from_secs(5), portal_fut).await {
            Ok(Ok(res)) => {
                response = Some(res);
                break;
            }
            Ok(Err(err)) => {
                let err_msg = err.to_string();
                // 若探测到 Portal 正在处理上一轮截屏（常见于用户快速连按快捷键），执行短暂退避重试
                if err_msg.contains("ongoing operation") && attempt < max_retries {
                    tracing::warn!(
                        "检测到 Portal 正在处理先前进发的截屏请求，等待 {}ms 后进行第 {}/{} 次重试...",
                        attempt * 150,
                        attempt + 1,
                        max_retries
                    );
                    tokio::time::sleep(Duration::from_millis(attempt as u64 * 150)).await;
                    last_err = Some(err);
                    continue;
                }
                return Err(err);
            }
            Err(_) => {
                return Err(PlatformError::Portal {
                    message: "等待 XDG Desktop Portal 响应超时".to_string(),
                });
            }
        }
    }

    let response = match response {
        Some(res) => res,
        None => {
            return Err(last_err.unwrap_or_else(|| PlatformError::Portal {
                message: "请求 XDG Desktop Portal 截屏失败".to_string(),
            }));
        }
    };

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
    let decode_path = file_path.clone();
    let rgba_img = tokio::task::spawn_blocking(move || {
        let bytes = fs::read(&decode_path).map_err(|e| PlatformError::Io {
            path: decode_path.to_string_lossy().to_string(),
            source: e,
        })?;
        if let Err(e) = fs::remove_file(&decode_path) {
            tracing::warn!("清理 Portal 临时图片文件失败: {e}");
        }
        image::load_from_memory(&bytes)
            .map(|dynamic_img| dynamic_img.into_rgba8())
            .map_err(|e| PlatformError::ImageDecode {
                message: format!(
                    "Portal 返回内容无法按受支持的图像格式解码，字节数: {}，原因: {e}",
                    bytes.len()
                ),
            })
    })
    .await
    .map_err(|error| PlatformError::Portal {
        message: format!("Portal 图像解码任务异常: {error}"),
    })??;
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

    // `send()` 会等待用户在 Portal 对话框作出选择；必须设置上限，
    // 否则对话框无人应答时保存任务会永久挂起并拖住守护进程停机。
    let dialog = tokio::time::timeout(PORTAL_DIALOG_TIMEOUT, request.send())
        .await
        .map_err(|_| PlatformError::Portal {
            message: format!(
                "等待保存对话框结果超时（{} 秒）",
                PORTAL_DIALOG_TIMEOUT.as_secs()
            ),
        })?
        .map_err(|error| PlatformError::Portal {
            message: format!("请求系统保存对话框失败: {error}"),
        })?;

    // Portal 将用户关闭对话框视为请求被取消而非故障，必须与真实错误区分：
    // 取消是正常用户操作，调用方据此保留当前界面而不提示保存失败。
    let files = match dialog.response() {
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
