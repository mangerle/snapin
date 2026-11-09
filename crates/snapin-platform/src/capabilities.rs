//! 系统平台能力探测与全局运行期缓存单例。
//!
//! 在程序启动时预先对当前桌面环境（Wayland 会话与 GNOME Mutter 扩展）执行一次系统能力探测，
//! 并缓存为全局只读快照，避免业务路径（如设置中心状态栏）重复发起
//! 无效的 D-Bus 探测与超时降级。业务侧据此决定截屏走扩展原生通道还是 XDG Portal，
//! 以及设置中心展示的扩展部署状态。

#[cfg(target_os = "linux")]
use crate::dbus_session::get_session_connection;
use arc_swap::ArcSwapOption;
use std::sync::{Arc, OnceLock};
#[cfg(target_os = "linux")]
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
#[cfg(target_os = "linux")]
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

#[cfg(not(target_os = "linux"))]
async fn probe_gnome_extension() -> bool {
    false
}

/// 判断接口自省 XML 中是否声明了 Snapin 特权接口
#[cfg(target_os = "linux")]
fn xml_declares_snapin_interface(xml: &str) -> bool {
    xml.contains("org.gnome.Shell.Extensions.Snapin")
}

static GLOBAL_CAPABILITIES: OnceLock<ArcSwapOption<PlatformCapabilities>> = OnceLock::new();

/// 初始化或获取全局平台能力快照（异步）。
///
/// 能力快照是典型读多写极少的发布数据，使用 `ArcSwapOption` 原子替换指针。
/// 业务读取路径只做一次无锁快照加载，不会与探测或刷新流程争用全局读写锁。
pub async fn init_platform_capabilities() -> Arc<PlatformCapabilities> {
    let caps = Arc::new(PlatformCapabilities::detect().await);
    GLOBAL_CAPABILITIES
        .get_or_init(ArcSwapOption::empty)
        .store(Some(caps.clone()));
    caps
}

/// 重新刷新平台能力快照（例如用户在设置中心重新部署或重载了扩展后调用）。
pub async fn refresh_platform_capabilities() -> Arc<PlatformCapabilities> {
    let caps = Arc::new(PlatformCapabilities::detect().await);
    GLOBAL_CAPABILITIES
        .get_or_init(ArcSwapOption::empty)
        .store(Some(caps.clone()));
    caps
}

/// 按需复检 GNOME 扩展在线状态并原子更新全局快照。
///
/// # 设计原理
///
/// 守护进程与 GNOME Shell 在登录阶段并行启动，扩展导出 D-Bus 对象的时间晚于
/// 守护进程完成首次能力探测是常见情形。若把这次“暂时离线”永久缓存，后续每次
/// 截图都会退化到 XDG Portal 通道；而 GNOME 的 Portal 后端会以 `flash=true`
/// 调用 `Shell.Screenshot`，使整屏出现一次快门白闪。这里在真正需要原生通道前
/// 复检一次：扩展一旦就绪立即恢复零白闪通道，无需用户重启守护进程。
///
/// 复检只替换扩展在线标志，不重复探测桌面环境；扩展本就离线时仅产生一次快速
/// D-Bus 往返，也不会重复输出启动阶段的 INFO 能力摘要。
///
/// # 返回
///
/// 返回复检后的最新快照；缓存已显示扩展在线时直接返回，不发起额外 D-Bus 调用。
#[cfg(target_os = "linux")]
pub(crate) async fn revalidate_gnome_extension_capability() -> Arc<PlatformCapabilities> {
    let current = get_platform_capabilities();
    if current.gnome_extension_active {
        return current;
    }

    let online = probe_gnome_extension().await;
    if !online {
        tracing::debug!("复检 GNOME 特权扩展仍不在线，本次继续使用降级抓屏通道");
        return current;
    }

    tracing::info!("复检确认 GNOME 特权扩展已就绪，恢复原生零白闪截屏通道");
    let updated = Arc::new(apply_extension_probe(&current, online));
    GLOBAL_CAPABILITIES
        .get_or_init(ArcSwapOption::empty)
        .store(Some(updated.clone()));
    updated
}

/// 构造仅替换扩展在线标志的能力快照，保留既有桌面环境探测结果。
#[cfg(target_os = "linux")]
fn apply_extension_probe(current: &PlatformCapabilities, online: bool) -> PlatformCapabilities {
    PlatformCapabilities {
        is_wayland: current.is_wayland,
        desktop_env: current.desktop_env.clone(),
        gnome_extension_active: online,
    }
}

/// 获取当前已缓存的全局平台能力快照。
///
/// 若尚未显式初始化，将执行同步快速启发式推断作为兜底。
pub fn get_platform_capabilities() -> Arc<PlatformCapabilities> {
    GLOBAL_CAPABILITIES
        .get()
        .and_then(|cell| cell.load_full())
        .unwrap_or_else(|| Arc::new(fallback_capabilities()))
}

/// 构建未初始化时的同步兜底能力快照。
fn fallback_capabilities() -> PlatformCapabilities {
    let (is_wayland, desktop_env) = probe_desktop_environment();
    PlatformCapabilities {
        is_wayland,
        desktop_env,
        gnome_extension_active: false,
    }
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
    #[cfg(target_os = "linux")]
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
    fn test_capabilities_fallback_is_offline() {
        let caps = fallback_capabilities();
        assert!(!caps.gnome_extension_active);
        assert!(!caps.desktop_env.is_empty());
    }

    /// 复检只允许改写扩展在线标志，桌面环境与 Wayland 结论必须原样保留
    #[cfg(target_os = "linux")]
    #[test]
    fn test_apply_extension_probe_only_updates_extension_flag() {
        let current = PlatformCapabilities {
            is_wayland: true,
            desktop_env: "ubuntu:GNOME".to_string(),
            gnome_extension_active: false,
        };

        let online = apply_extension_probe(&current, true);
        assert!(online.gnome_extension_active);
        assert!(online.is_wayland);
        assert_eq!(online.desktop_env, "ubuntu:GNOME");

        let offline = apply_extension_probe(&online, false);
        assert!(!offline.gnome_extension_active);
        assert_eq!(offline.desktop_env, "ubuntu:GNOME");
    }
}
