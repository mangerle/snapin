//! 全局复用的 D-Bus 会话总线连接单例池。

use crate::error::PlatformError;
use tokio::sync::OnceCell;
use zbus::Connection;

static SESSION_CONNECTION: OnceCell<Connection> = OnceCell::const_new();

/// 获取全局唯一的 D-Bus Session 会话总线连接引用
///
/// 遵循单例连接池复用原则，避免每次发送桌面通知或调用 GNOME Shell 扩展时
/// 重复进行昂贵的 D-Bus 握手、鉴权与套接字创建。
pub async fn get_session_connection() -> Result<&'static Connection, PlatformError> {
    SESSION_CONNECTION
        .get_or_try_init(|| async {
            Connection::session()
                .await
                .map_err(|e| PlatformError::DBus {
                    service: "D-Bus Session Bus".to_string(),
                    message: e.to_string(),
                })
        })
        .await
}
