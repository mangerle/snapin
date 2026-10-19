//! 与配套 GNOME Shell 扩展通信的 D-Bus 客户端实现。
//!
//! 通过向 Mutter 内部暴露的 D-Bus 接口发送指定贴图窗口的唯一标题，
//! 请求合成器调用 `meta_window.make_above()` 赋予其最高层级置顶特权。

use crate::error::SnapinError;
use tokio::sync::OnceCell;
use zbus::Connection;

const EXT_BUS_NAME: &str = "org.gnome.Shell";
const EXT_OBJECT_PATH: &str = "/org/gnome/Shell/Extensions/Snapin";
const EXT_INTERFACE: &str = "org.gnome.Shell.Extensions.Snapin";

static SESSION_CONNECTION: OnceCell<Connection> = OnceCell::const_new();

/// 获取或初始化全局复用的 D-Bus Session 会话总线单例连接
async fn get_session_connection() -> Result<&'static Connection, SnapinError> {
    SESSION_CONNECTION
        .get_or_try_init(|| async {
            Connection::session()
                .await
                .map_err(|e| SnapinError::DBusError {
                    service: "D-Bus Session Bus".to_string(),
                    source: e,
                })
        })
        .await
}

/// 请求配套 GNOME Shell 扩展将指定标题的贴图窗口置顶
pub async fn pin_window_above(window_title: &str) -> Result<bool, SnapinError> {
    let connection = get_session_connection().await?;

    tracing::debug!("正在请求 GNOME 扩展置顶窗口，标题: {}", window_title);

    let msg = connection
        .call_method(
            Some(EXT_BUS_NAME),
            EXT_OBJECT_PATH,
            Some(EXT_INTERFACE),
            "PinWindowByTitle",
            &(window_title),
        )
        .await
        .map_err(|e| SnapinError::DBusError {
            service: EXT_BUS_NAME.to_string(),
            source: e,
        })?;

    let reply: (bool,) = msg
        .body()
        .deserialize()
        .map_err(|e| SnapinError::DBusError {
            service: EXT_BUS_NAME.to_string(),
            source: e,
        })?;

    let success = reply.0;
    if success {
        tracing::info!("贴图窗口成功置顶: {}", window_title);
    } else {
        tracing::warn!("GNOME 扩展未能找到该标题的窗口进行置顶: {}", window_title);
    }
    Ok(success)
}

/// 请求配套 GNOME Shell 扩展执行毫秒级原生无白闪全屏截屏
pub async fn capture_fullscreen_extension(target_path: &str) -> Result<bool, SnapinError> {
    let connection = get_session_connection().await?;

    tracing::debug!("正在向 GNOME 扩展请求原生极速全屏截屏: {}", target_path);

    let msg = connection
        .call_method(
            Some(EXT_BUS_NAME),
            EXT_OBJECT_PATH,
            Some(EXT_INTERFACE),
            "CaptureFullscreen",
            &(target_path),
        )
        .await
        .map_err(|e| SnapinError::DBusError {
            service: EXT_BUS_NAME.to_string(),
            source: e,
        })?;

    let reply: (bool,) = msg
        .body()
        .deserialize()
        .map_err(|e| SnapinError::DBusError {
            service: EXT_BUS_NAME.to_string(),
            source: e,
        })?;

    Ok(reply.0)
}
