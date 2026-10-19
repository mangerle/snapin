//! 基于 Freedesktop 标准的桌面通知发送实现。

use snapin_core::error::SnapinError;
use std::collections::HashMap;
use zbus::zvariant::Value;
use zbus::Connection;

const NOTIFICATION_BUS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATION_PATH: &str = "/org/freedesktop/Notifications";
const NOTIFICATION_INTERFACE: &str = "org.freedesktop.Notifications";

/// 向系统通知中心发送一条自消除的气泡通知
pub async fn send_desktop_notification(summary: &str, body: &str) -> Result<u32, SnapinError> {
    let connection = Connection::session()
        .await
        .map_err(|e| SnapinError::DBusError {
            service: NOTIFICATION_BUS_NAME.to_string(),
            message: e.to_string(),
        })?;

    let app_name = "Snapin";
    let replaces_id: u32 = 0;
    let app_icon = "camera-photo";
    let actions: Vec<String> = Vec::new();
    let hints: HashMap<String, Value<'_>> = HashMap::new();
    let timeout_ms: i32 = 2500; // 2.5 秒后自动淡出

    let msg = connection
        .call_method(
            Some(NOTIFICATION_BUS_NAME),
            NOTIFICATION_PATH,
            Some(NOTIFICATION_INTERFACE),
            "Notify",
            &(
                app_name,
                replaces_id,
                app_icon,
                summary,
                body,
                actions,
                hints,
                timeout_ms,
            ),
        )
        .await
        .map_err(|e| SnapinError::DBusError {
            service: NOTIFICATION_BUS_NAME.to_string(),
            message: e.to_string(),
        })?;

    let reply: (u32,) = msg
        .body()
        .deserialize()
        .map_err(|e| SnapinError::DBusError {
            service: NOTIFICATION_BUS_NAME.to_string(),
            message: e.to_string(),
        })?;

    Ok(reply.0)
}
