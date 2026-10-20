//! 与配套 GNOME Shell 扩展通信的 D-Bus 客户端实现。
//!
//! 通过向 Mutter 内部暴露的 D-Bus 接口发送指定贴图窗口的唯一标题，
//! 请求合成器调用 `meta_window.make_above()` 赋予其最高层级置顶特权。
//! 所有方法共用统一的超时、错误映射与反序列化逻辑。

use crate::dbus_session::get_session_connection;
use crate::error::PlatformError;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;
use tokio::time::timeout;
use zbus::zvariant::Type;

const EXT_BUS_NAME: &str = "org.gnome.Shell";
const EXT_OBJECT_PATH: &str = "/org/gnome/Shell/Extensions/Snapin";
const EXT_INTERFACE: &str = "org.gnome.Shell.Extensions.Snapin";
const EXT_DBUS_TIMEOUT: Duration = Duration::from_secs(3);

/// 统一调用扩展 D-Bus 方法并反序列化返回值。
async fn call_extension<A, R>(method: &str, arguments: &A) -> Result<R, PlatformError>
where
    A: Serialize + Type + Sync,
    R: DeserializeOwned + Type,
{
    let connection = get_session_connection().await?;
    let call = connection.call_method(
        Some(EXT_BUS_NAME),
        EXT_OBJECT_PATH,
        Some(EXT_INTERFACE),
        method,
        arguments,
    );

    let message = timeout(EXT_DBUS_TIMEOUT, call)
        .await
        .map_err(|_| PlatformError::DBus {
            service: EXT_BUS_NAME.to_string(),
            message: format!("GNOME 扩展方法 {method} 调用超时"),
        })?
        .map_err(|error| PlatformError::DBus {
            service: EXT_BUS_NAME.to_string(),
            message: format!("GNOME 扩展方法 {method} 调用失败: {error}"),
        })?;

    message
        .body()
        .deserialize::<R>()
        .map_err(|error| PlatformError::DBus {
            service: EXT_BUS_NAME.to_string(),
            message: format!("GNOME 扩展方法 {method} 返回值解析失败: {error}"),
        })
}

/// 请求配套 GNOME Shell 扩展将指定标题的贴图窗口置顶
pub async fn pin_window_above(window_title: &str) -> Result<bool, PlatformError> {
    tracing::debug!("正在请求 GNOME 扩展置顶窗口，标题: {}", window_title);
    let (success,): (bool,) = call_extension("PinWindowByTitle", &(window_title)).await?;

    if success {
        tracing::info!("贴图窗口成功置顶: {}", window_title);
    } else {
        tracing::warn!("GNOME 扩展未能找到该标题的窗口进行置顶: {}", window_title);
    }
    Ok(success)
}

/// 请求配套 GNOME Shell 扩展执行毫秒级原生无白闪全屏截屏
pub async fn capture_fullscreen_extension(target_path: &str) -> Result<bool, PlatformError> {
    tracing::debug!("正在向 GNOME 扩展请求原生极速全屏截屏: {}", target_path);
    let (success,): (bool,) = call_extension("CaptureFullscreen", &(target_path)).await?;
    Ok(success)
}

/// 请求配套 GNOME Shell 扩展从 Mutter ClutterStage 直接抓取原始物理像素（跳过 PNG 压缩）
///
/// 成功捕获后返回原始图像分辨率 `Some((width, height))`
pub async fn capture_fullscreen_extension_raw(
    target_path: &str,
) -> Result<Option<(u32, u32)>, PlatformError> {
    tracing::debug!(
        "正在向 GNOME 扩展请求零拷贝原始像素全屏截屏: {}",
        target_path
    );
    let (width, height, success): (u32, u32, bool) =
        call_extension("CaptureFullscreenRaw", &(target_path)).await?;

    if success && width > 0 && height > 0 {
        Ok(Some((width, height)))
    } else {
        Ok(None)
    }
}
