//! 与配套 GNOME Shell 扩展通信的 D-Bus 客户端实现。
//!
//! 提供两类必须由合成器特权完成的能力：贴图窗口的绝对定位与最高层级置顶，以及经
//! `Shell.Screenshot` 内部对象直写文件、无快门白闪的原生全屏截屏。
//!
//! 原生截屏只走"写入文件流"这一种形态：早先还有一条在 shell 进程内 `read_pixels` 直取
//! 原始像素的通道，其数组返回值在 GNOME 50 的 GJS 上会命中编解码断言并以 SIGABRT 终止
//! gnome-shell（按一次截图即登出整个会话），已永久移除。扩展不在线时由 portal 兜底。
//! 所有方法共用统一的超时、错误映射与反序列化逻辑。

use crate::dbus_session::get_session_connection;
use crate::error::PlatformError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::timeout;
use zbus::zvariant::Type;

const EXT_BUS_NAME: &str = "org.gnome.Shell";
const EXT_OBJECT_PATH: &str = "/org/gnome/Shell/Extensions/Snapin";
const EXT_INTERFACE: &str = "org.gnome.Shell.Extensions.Snapin";
const EXT_DBUS_TIMEOUT: Duration = Duration::from_secs(3);

/// 扩展返回的窗口矩形（物理像素，已按各窗口所在显示器的缩放比换算）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 扩展返回的原始窗口几何：逻辑坐标 + 所在显示器缩放比。
#[derive(Debug, Deserialize)]
struct RawWindowRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    #[serde(default = "default_window_scale")]
    scale: f64,
}

fn default_window_scale() -> f64 {
    1.0
}

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

/// 请求扩展显示或隐藏顶栏托盘。
///
/// 退出 Snapin 时只隐藏托盘、不停用扩展：扩展被停用后 GNOME 45+ 在本会话内无法重新加载，
/// 贴图定位与原生截屏会一并失效直到注销重登。守护进程重新启动时会再次调用本方法恢复托盘。
///
/// # Errors
///
/// 会话总线不可用、扩展未部署、方法调用超时或返回值解析失败时返回 [`PlatformError::DBus`]。
pub async fn set_extension_tray_visible(visible: bool) -> Result<(), PlatformError> {
    tracing::debug!(
        "正在请求 GNOME 扩展{}顶栏托盘",
        if visible { "显示" } else { "隐藏" }
    );
    let (applied,): (bool,) = call_extension("SetTrayVisible", &(visible)).await?;
    if !applied {
        tracing::warn!(
            "GNOME 扩展未确认{}顶栏托盘，扩展版本可能过旧",
            if visible { "显示" } else { "隐藏" }
        );
    }
    Ok(())
}

/// 请求配套 GNOME Shell 扩展执行毫秒级原生无白闪全屏截屏，结果写入 `target_path` 指向的 PNG。
///
/// # Errors
///
/// 会话总线不可用、扩展未部署、方法调用超时或返回值解析失败时返回 [`PlatformError::DBus`]。
pub async fn capture_fullscreen_extension(target_path: &str) -> Result<bool, PlatformError> {
    tracing::debug!("正在向 GNOME 扩展请求原生极速全屏截屏: {}", target_path);
    let (success,): (bool,) = call_extension("CaptureFullscreen", &(target_path)).await?;
    Ok(success)
}

/// 请求扩展枚举当前活动工作区可见窗口的物理像素矩形。
///
/// 返回顺序为窗口堆叠顺序（底层到顶层），调用方应反向遍历以优先命中顶层窗口。
/// 扩展不可用、会话总线异常或返回 JSON 无法解析时返回错误，由截图流程降级为
/// 普通区域截图。
///
/// # Errors
///
/// 扩展未部署、D-Bus 调用超时或窗口 JSON 解析失败时返回 [`PlatformError::DBus`]。
pub async fn list_windows() -> Result<Vec<WindowRect>, PlatformError> {
    let (json,): (String,) = call_extension("ListWindows", &()).await?;
    parse_windows_json(&json)
}

/// 解析扩展返回的窗口 JSON，并把逻辑坐标换算为物理像素。
fn parse_windows_json(json: &str) -> Result<Vec<WindowRect>, PlatformError> {
    let raw: Vec<RawWindowRect> =
        serde_json::from_str(json).map_err(|error| PlatformError::DBus {
            service: EXT_BUS_NAME.to_string(),
            message: format!("解析 GNOME 扩展窗口列表 JSON 失败: {error}"),
        })?;

    let mut rects = Vec::with_capacity(raw.len());
    for window in raw {
        let scale = if window.scale.is_finite() && window.scale > 0.0 {
            window.scale
        } else {
            1.0
        };
        let x = window.x * scale;
        let y = window.y * scale;
        let width = window.width * scale;
        let height = window.height * scale;
        if !x.is_finite() || !y.is_finite() || !width.is_finite() || !height.is_finite() {
            continue;
        }
        if width <= 1.0 || height <= 1.0 {
            continue;
        }
        rects.push(WindowRect {
            x: x.round() as i32,
            y: y.round() as i32,
            width: width.round().min(u32::MAX as f64) as u32,
            height: height.round().min(u32::MAX as f64) as u32,
        });
    }
    Ok(rects)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// 窗口 JSON 解析应将逻辑坐标按显示器缩放比换算，并过滤空尺寸窗口
    #[test]
    fn test_parse_windows_json_scales_and_filters() {
        let json = r#"[
            {"x": 100.0, "y": 50.0, "width": 200.0, "height": 100.0, "scale": 2.0},
            {"x": 0.0, "y": 0.0, "width": 0.0, "height": 100.0, "scale": 1.0}
        ]"#;
        let rects = parse_windows_json(json).expect("窗口 JSON 应解析成功");
        assert_eq!(rects.len(), 1);
        assert_eq!(
            rects[0],
            WindowRect {
                x: 200,
                y: 100,
                width: 400,
                height: 200,
            }
        );
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
