//! 与配套 GNOME Shell 扩展通信的 D-Bus 客户端实现。
//!
//! Wayland 协议禁止客户端自行摆放或提级窗口，贴图窗口的绝对定位与最高层级置顶只能由拥有
//! 合成器特权的配套扩展代为执行；扩展同时提供毫秒级原生无白闪全屏截屏能力。
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

/// 请求配套 GNOME Shell 扩展将指定贴图窗口置顶并精确放置到给定物理像素坐标。
///
/// 返回值语义：`true` 表示窗口已映射并立即完成放置；`false` 表示窗口尚未映射，
/// 扩展已将该坐标登记为延迟请求，待 `window-created` 触发时自动生效，因此调用方不应视为失败。
///
/// # Errors
///
/// 会话总线不可用、扩展未部署、方法调用超时或返回值解析失败时返回 [`PlatformError::DBus`]。
pub async fn place_paster_window(title: &str, x: i32, y: i32) -> Result<bool, PlatformError> {
    tracing::debug!("正在请求 GNOME 扩展放置贴图窗口: {title}，坐标: ({x}, {y})");
    let (placed,): (bool,) = call_extension("PlaceWindowByTitle", &(title, x, y)).await?;
    Ok(placed)
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

#[cfg(test)]
mod tests {
    /// 提取本文件中实际调用的扩展 D-Bus 方法名。
    ///
    /// 直接扫描源码而非手写清单，使测试随客户端代码单向收敛，新增方法无需同步维护断言列表。
    fn called_methods() -> Vec<String> {
        const MARKER: &str = "call_extension(\"";
        include_str!("extension_client.rs")
            .split(MARKER)
            .skip(1)
            .filter_map(|segment| segment.split('"').next().map(str::to_owned))
            .collect()
    }

    /// 校验 Rust 侧调用的每个 D-Bus 方法都在配套扩展里既声明了接口又实现了方法体。
    ///
    /// 跨语言边界不受类型系统约束，方法名或签名一旦单边改动只会在运行期静默失败并退化为
    /// 降级路径，因此以源码文本比对锁定两端契约。
    #[test]
    fn test_extension_declares_all_called_methods() {
        let methods = called_methods();
        assert!(!methods.is_empty(), "应至少扫描到一个扩展 D-Bus 调用");

        let js = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../extension/extension.js"
        ))
        .expect("配套 GNOME 扩展源码应随仓库一同检出");

        for method in methods {
            assert!(
                js.contains(&format!("<method name=\"{method}\">")),
                "扩展接口定义缺少方法: {method}"
            );
            assert!(
                js.contains(&format!("{method}(")),
                "扩展实现缺少方法体: {method}"
            );
        }
    }
}
