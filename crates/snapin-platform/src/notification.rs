//! 基于 Freedesktop 标准的桌面通知发送实现。

use crate::dbus_session::get_session_connection;
use crate::error::PlatformError;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
use zbus::zvariant::Value;

const NOTIFICATION_BUS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATION_PATH: &str = "/org/freedesktop/Notifications";
const NOTIFICATION_INTERFACE: &str = "org.freedesktop.Notifications";

/// 向系统通知中心发送一条自消除的气泡通知
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
