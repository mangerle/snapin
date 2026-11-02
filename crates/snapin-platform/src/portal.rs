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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// 等待用户在保存对话框中作出选择的超时上限。
///
/// ashpd 0.8 的 `send()` 内部会等待 Portal 的 Response 信号，因此该超时覆盖
/// “建立请求 + 用户交互”的完整过程；没有上限时，用户打开对话框后离开会让
/// 保存任务永久挂起并拖住守护进程停机。
const PORTAL_DIALOG_TIMEOUT: Duration = Duration::from_secs(300);

/// 允许预登记截屏授权的应用标识列表。
///
/// 禁止出现空字符串：在 PermissionStore 中空 app_id 通常代表“无 app id 客户端”，
/// 写入它等于给所有无法标识自身身份的宿主程序长期放行截屏，绕过 Portal 的用户授权语义。
const PORTAL_PERMISSION_APP_IDS: &[&str] = &["snapin", "snapin.desktop"];

/// 单次 PermissionStore 调用的超时上限
const PORTAL_PERMISSION_TIMEOUT: Duration = Duration::from_secs(2);

/// 本轮进程是否已成功预登记截屏授权，避免每次截图重复写后端权限存储。
static PORTAL_PERMISSION_ENSURED: AtomicBool = AtomicBool::new(false);

/// 自愈确保当前应用在 XDG Desktop Portal 权限存储区具备合法的截屏授权。
///
/// 在 GNOME 50 Wayland 环境下，如果后台进程触发截屏且无前台焦点，
/// Portal 会因尝试弹窗而报错 `AccessDenied: Only the focused app is allowed to show a system access dialog`。
/// 通过在系统 PermissionStore 中预登记受信任授权，Portal 将直接交付帧缓冲而无需调起交互授权弹窗。
///
/// # 风险与降级
///
/// `org.freedesktop.impl.portal.PermissionStore` 属于 Portal 后端内部接口，并非稳定的
/// 客户端契约。这里仅对明确的 Snapin 应用标识做一次性登记，绝不写入空 app_id；
/// 调用失败会返回错误，由调用方记录告警并回退到 Portal 的常规交互授权路径。
///
/// # Errors
///
/// 会话总线不可用、PermissionStore 拒绝调用或调用超时时返回 [`PlatformError::DBus`]。
pub async fn ensure_portal_screenshot_permission() -> Result<(), PlatformError> {
    if PORTAL_PERMISSION_ENSURED.load(Ordering::Acquire) {
        return Ok(());
    }

    let connection = get_session_connection().await?;
    let mut failures: Vec<String> = Vec::with_capacity(PORTAL_PERMISSION_APP_IDS.len());

    for app_id in PORTAL_PERMISSION_APP_IDS {
        let arguments = ("screenshot", true, "screenshot", *app_id, &["yes"]);
        let call = connection.call_method(
            Some("org.freedesktop.impl.portal.PermissionStore"),
            "/org/freedesktop/impl/portal/PermissionStore",
            Some("org.freedesktop.impl.portal.PermissionStore"),
            "SetPermission",
            &arguments,
        );

        match tokio::time::timeout(PORTAL_PERMISSION_TIMEOUT, call).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => failures.push(format!("{app_id}: {error}")),
            Err(_) => failures.push(format!("{app_id}: 调用超时")),
        }
    }

    if failures.is_empty() {
        PORTAL_PERMISSION_ENSURED.store(true, Ordering::Release);
        return Ok(());
    }

    Err(PlatformError::DBus {
        service: "org.freedesktop.impl.portal.PermissionStore".to_string(),
        message: format!("预登记截屏授权失败: {}", failures.join("；")),
    })
}

/// 请求 XDG Desktop Portal 执行静默全屏截屏（非交互模式）。
///
/// 具备退避防抖重试机制（解决快捷键并发按压时 Portal 的 `ongoing operation` 冲突），
/// 并在调用前自动确保自愈权限，避免无焦点时抛出 `AccessDenied`。
pub async fn capture_fullscreen_portal() -> Result<RgbaImage, PlatformError> {
    tracing::debug!("正在向 XDG Desktop Portal 请求全屏快照...");

    // 预先确保后台静默截屏权限，防止被 GNOME 50 拦截；
    // 内部接口不可用时记录告警并继续走 Portal 常规交互授权路径。
    if let Err(error) = ensure_portal_screenshot_permission().await {
        tracing::warn!("预登记 Portal 截屏权限失败，回退到常规授权路径: {error}");
    }

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

/// 打开配置文件导入对话框。
///
/// # Errors
///
/// Portal 不可用、超时或返回 URI 无法转换时返回 [`PlatformError::Portal`]。
pub async fn pick_config_import_path() -> Result<Option<PathBuf>, PlatformError> {
    let request = SelectedFiles::open_file()
        .title("导入 Snapin 配置")
        .accept_label("导入")
        .modal(true)
        .filter(FileFilter::new("Snapin 配置").glob("*.toml"));
    let dialog = tokio::time::timeout(PORTAL_DIALOG_TIMEOUT, request.send())
        .await
        .map_err(|_| PlatformError::Portal {
            message: format!(
                "等待配置导入对话框结果超时（{} 秒）",
                PORTAL_DIALOG_TIMEOUT.as_secs()
            ),
        })?
        .map_err(|error| PlatformError::Portal {
            message: format!("请求配置导入对话框失败: {error}"),
        })?;
    let files = match dialog.response() {
        Ok(files) => files,
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => return Ok(None),
        Err(error) => {
            return Err(PlatformError::Portal {
                message: format!("等待配置导入对话框结果失败: {error}"),
            })
        }
    };
    let Some(uri) = files.uris().first() else {
        return Ok(None);
    };
    uri.to_file_path()
        .map(Some)
        .map_err(|_| PlatformError::Portal {
            message: format!("配置导入对话框返回的 URI 无法转换为本地路径: {uri}"),
        })
}

/// 打开配置文件导出对话框。
///
/// # Errors
///
/// Portal 不可用、超时或返回 URI 无法转换时返回 [`PlatformError::Portal`]。
pub async fn pick_config_export_path() -> Result<Option<PathBuf>, PlatformError> {
    let request = SelectedFiles::save_file()
        .title("导出 Snapin 配置")
        .accept_label("导出")
        .current_name("snapin-config.toml")
        .modal(true)
        .filter(FileFilter::new("Snapin 配置").glob("*.toml"));
    let dialog = tokio::time::timeout(PORTAL_DIALOG_TIMEOUT, request.send())
        .await
        .map_err(|_| PlatformError::Portal {
            message: format!(
                "等待配置导出对话框结果超时（{} 秒）",
                PORTAL_DIALOG_TIMEOUT.as_secs()
            ),
        })?
        .map_err(|error| PlatformError::Portal {
            message: format!("请求配置导出对话框失败: {error}"),
        })?;
    let files = match dialog.response() {
        Ok(files) => files,
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => return Ok(None),
        Err(error) => {
            return Err(PlatformError::Portal {
                message: format!("等待配置导出对话框结果失败: {error}"),
            })
        }
    };
    let Some(uri) = files.uris().first() else {
        return Ok(None);
    };
    uri.to_file_path()
        .map(Some)
        .map_err(|_| PlatformError::Portal {
            message: format!("配置导出对话框返回的 URI 无法转换为本地路径: {uri}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 预登记授权只能针对明确应用标识，绝不能出现空 app_id 或重复项。
    #[test]
    fn test_permission_app_ids_are_non_empty_and_unique() {
        assert!(!PORTAL_PERMISSION_APP_IDS.is_empty());
        let unique: HashSet<&str> = PORTAL_PERMISSION_APP_IDS.iter().copied().collect();
        assert_eq!(unique.len(), PORTAL_PERMISSION_APP_IDS.len());
        for app_id in PORTAL_PERMISSION_APP_IDS {
            assert!(!app_id.trim().is_empty(), "app_id 不得为空");
        }
    }
}
