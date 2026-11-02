//! 基于 Freedesktop 标准的桌面通知发送实现。

use crate::error::PlatformError;

#[cfg(target_os = "linux")]
use crate::dbus_session::get_session_connection;
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(target_os = "linux")]
use tokio::time::timeout;
#[cfg(target_os = "linux")]
use zbus::zvariant::Value;

#[cfg(target_os = "linux")]
const NOTIFICATION_BUS_NAME: &str = "org.freedesktop.Notifications";
#[cfg(target_os = "linux")]
const NOTIFICATION_PATH: &str = "/org/freedesktop/Notifications";
#[cfg(target_os = "linux")]
const NOTIFICATION_INTERFACE: &str = "org.freedesktop.Notifications";

/// 向系统通知中心发送一条自消除的气泡通知
#[cfg(target_os = "linux")]
/// # Errors
///
/// 通知服务不可用、D-Bus 调用失败或响应超时时返回 [`PlatformError`]。
pub async fn send_desktop_notification(summary: &str, body: &str) -> Result<u32, PlatformError> {
    let connection = get_session_connection().await?;

    let app_name = "Snapin";
    let replaces_id: u32 = 0;
    let app_icon = "camera-photo";
    let actions: Vec<String> = Vec::new();
    let hints: HashMap<String, Value<'_>> = HashMap::new();
    let timeout_ms: i32 = 2500; // 2.5 秒后自动淡出

    let arguments = (
        app_name,
        replaces_id,
        app_icon,
        summary,
        body,
        actions,
        hints,
        timeout_ms,
    );
    let call = connection.call_method(
        Some(NOTIFICATION_BUS_NAME),
        NOTIFICATION_PATH,
        Some(NOTIFICATION_INTERFACE),
        "Notify",
        &arguments,
    );

    let msg = timeout(Duration::from_secs(3), call)
        .await
        .map_err(|_| PlatformError::DBus {
            service: NOTIFICATION_BUS_NAME.to_string(),
            message: "发送桌面通知超时".to_string(),
        })?
        .map_err(|e| PlatformError::DBus {
            service: NOTIFICATION_BUS_NAME.to_string(),
            message: e.to_string(),
        })?;

    let reply: (u32,) = msg.body().deserialize().map_err(|e| PlatformError::DBus {
        service: NOTIFICATION_BUS_NAME.to_string(),
        message: e.to_string(),
    })?;

    Ok(reply.0)
}

/// 非 Linux 平台暂未接入系统原生通知中心，显式返回不支持。
///
/// 调用方都把通知视为非关键副作用并降级为调试日志；这里不再伪造 `Ok(0)`，
/// 避免上层误以为通知已经送达。
#[cfg(not(target_os = "linux"))]
pub async fn send_desktop_notification(_summary: &str, _body: &str) -> Result<u32, PlatformError> {
    Err(PlatformError::Unsupported {
        capability: "系统桌面通知".to_string(),
    })
}
