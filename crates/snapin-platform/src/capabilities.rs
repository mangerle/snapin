//! 系统平台能力探测与全局运行期缓存单例。
//!
//! 在程序启动时预先对当前桌面环境（Wayland 会话与 GNOME Mutter 扩展）执行一次系统能力探测，
//! 并缓存为全局只读快照，避免业务路径（如设置中心状态栏）重复发起
//! 无效的 D-Bus 探测与超时降级。业务侧据此决定截屏走扩展原生通道还是 XDG Portal，
//! 以及设置中心展示的扩展部署状态。

use crate::dbus_session::get_session_connection;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

/// 系统底层平台能力快照
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    /// 是否运行在 Wayland 显示服务器下
    pub is_wayland: bool,
    /// 桌面环境名称（例如 GNOME, KDE, Sway 等）
    pub desktop_env: String,
    /// 配套 GNOME Shell Mutter 特权扩展是否在线就绪
    pub gnome_extension_active: bool,
}

impl PlatformCapabilities {
    /// 执行全系统能力异步探测
    pub async fn detect() -> Self {
        let (is_wayland, desktop_env) = probe_desktop_environment();
        let gnome_extension_active = probe_gnome_extension().await;

        tracing::info!(
            "系统平台能力探测完成: Wayland={}, 桌面={}, GNOME特权扩展在线={}",
            is_wayland,
            desktop_env,
            gnome_extension_active
        );

        Self {
            is_wayland,
            desktop_env,
            gnome_extension_active,
        }
    }
}

/// 探测当前会话是否为 Wayland 以及桌面环境标识
fn probe_desktop_environment() -> (bool, String) {
    let session_type = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_lowercase();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    let is_wayland = session_type == "wayland" || !wayland_display.is_empty();

    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("DESKTOP_SESSION"))
        .unwrap_or_else(|_| "Unknown".to_string());

    (is_wayland, desktop)
}

/// 快速轻量探测 GNOME Shell 扩展是否真正导出了 Snapin 特权接口。
///
/// 不能用 `Peer.Ping`：它只验证对象路径存在，扩展被停用后 shell 仍会在导出树里保留该节点，
/// 于是探测报告"在线"而 `PlaceWindowByTitle` 实际返回 UnknownMethod，贴图静默失去定位能力。
/// 改为请求接口自省并要求自省结果里出现本扩展的接口名，只有接口真的导出才算在线。
async fn probe_gnome_extension() -> bool {
    let Ok(connection) = get_session_connection().await else {
        return false;
    };

    let probe_fut = async {
        let reply = connection
            .call_method(
                Some("org.gnome.Shell"),
                "/org/gnome/Shell/Extensions/Snapin",
                Some("org.freedesktop.DBus.Introspectable"),
                "Introspect",
                &(),
            )
            .await
            .ok()?;
        let (xml,): (String,) = reply.body().deserialize().ok()?;
        Some(xml_declares_snapin_interface(&xml))
    };

    // 限制在 300 毫秒内响应，避免 D-Bus 未就绪导致阻塞
    matches!(
        tokio::time::timeout(Duration::from_millis(300), probe_fut).await,
        Ok(Some(true))
    )
}

/// 判断接口自省 XML 中是否声明了 Snapin 特权接口
fn xml_declares_snapin_interface(xml: &str) -> bool {
    xml.contains("org.gnome.Shell.Extensions.Snapin")
}

static GLOBAL_CAPABILITIES: OnceLock<RwLock<Arc<PlatformCapabilities>>> = OnceLock::new();

/// 初始化或获取全局平台能力快照（异步）
pub async fn init_platform_capabilities() -> Arc<PlatformCapabilities> {
    let caps = Arc::new(PlatformCapabilities::detect().await);
    let cell = GLOBAL_CAPABILITIES.get_or_init(|| RwLock::new(caps.clone()));
    if let Ok(mut lock) = cell.write() {
        *lock = caps.clone();
    }
    caps
}

/// 重新刷新平台能力快照（例如用户在设置中心重新部署或重载了扩展后调用）
pub async fn refresh_platform_capabilities() -> Arc<PlatformCapabilities> {
    let caps = Arc::new(PlatformCapabilities::detect().await);
    if let Some(cell) = GLOBAL_CAPABILITIES.get() {
        if let Ok(mut lock) = cell.write() {
            *lock = caps.clone();
        }
    } else {
        let _ = GLOBAL_CAPABILITIES.set(RwLock::new(caps.clone()));
    }
    caps
}

/// 获取当前已缓存的全局平台能力快照
///
/// 若尚未显式初始化，将执行同步快速启发式推断作为兜底
pub fn get_platform_capabilities() -> Arc<PlatformCapabilities> {
    if let Some(cell) = GLOBAL_CAPABILITIES.get() {
        if let Ok(lock) = cell.read() {
            return lock.clone();
        }
    }

    // 兜底同步构造
    let (is_wayland, desktop_env) = probe_desktop_environment();
    Arc::new(PlatformCapabilities {
        is_wayland,
        desktop_env,
        gnome_extension_active: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_environment_probe_fallback() {
        let (is_wayland, desktop) = probe_desktop_environment();
        // 确认不会触发 panic 且返回了有效字符串
        assert!(!desktop.is_empty());
        let _ = is_wayland;
    }

    /// 空节点（扩展已停用但路径仍在）必须判为离线，接口已导出才判为在线
    #[test]
    fn test_interface_probe_rejects_empty_node() {
        assert!(!xml_declares_snapin_interface(
            "node /org/gnome/Shell/Extensions/Snapin {\n};"
        ));
        let exported = r#"<node><interface name="org.gnome.Shell.Extensions.Snapin">
            <method name="PlaceWindowByTitle"/></interface></node>"#;
        assert!(xml_declares_snapin_interface(exported));
    }

    #[test]
    fn test_capabilities_fallback_singleton() {
        let caps = get_platform_capabilities();
        assert!(!caps.gnome_extension_active);
    }
}
