//! 跨平台鼠标输入与滚轮模拟。
//!
//! # 设计原理
//!
//! 在长截图录制期间，截图全屏窗口通常位于最顶层，拦截了操作系统向底层应用传递的原生鼠标事件。
//! 为实现 PixPin 风格的“在选区内滚动鼠标滚轮时，驱动底层目标软件同步滚动并自动拼接长图”，
//! 本模块向操作系统发送轻量级的鼠标滚轮输入事件。
//!
//! - Windows: 直接调用 Win32 原生 `mouse_event(MOUSEEVENTF_WHEEL)`；
//! - Linux: 优先调用 GNOME 扩展的 Clutter 虚拟指针，其次降级到 `ydotool` 或 `xdotool`；
//! - macOS: 通过 CoreGraphics 派发滚动滚轮事件；
//! - 其他平台: 接口保持一致，安全降级。

use crate::error::PlatformError;

#[cfg(target_os = "linux")]
use crate::capabilities::get_platform_capabilities;
#[cfg(target_os = "linux")]
use crate::command::output_with_timeout;
#[cfg(target_os = "linux")]
use crate::extension_client::simulate_mouse_wheel_extension;
#[cfg(target_os = "linux")]
use std::time::Duration;

/// 外部滚轮注入工具的最长等待时间，防止输入工具卡死长期占用阻塞线程。
#[cfg(target_os = "linux")]
const WHEEL_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

/// 异步模拟鼠标垂直滚轮滚动。
///
/// Linux 优先走 GNOME Shell 扩展的合成器内虚拟指针，不依赖 `/dev/uinput` 写权限；
/// 扩展不可用时再降级到外部输入工具。Windows 与 macOS 在阻塞线程调用原生输入 API。
///
/// # Errors
///
/// 当前平台没有可用滚轮注入通路、扩展调用失败，或阻塞任务异常结束时返回 [`PlatformError`]。
pub async fn simulate_mouse_wheel_async(delta_y: f32) -> Result<(), PlatformError> {
    if delta_y.abs() < f32::EPSILON {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        if get_platform_capabilities().gnome_extension_active {
            match simulate_mouse_wheel_extension(delta_y).await {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    tracing::debug!("GNOME 扩展拒绝注入滚轮事件，降级到外部输入工具");
                }
                Err(error) => {
                    tracing::debug!("GNOME 扩展滚轮注入失败，降级到外部输入工具: {error}");
                }
            }
        }
        tokio::task::spawn_blocking(move || simulate_linux_wheel(delta_y))
            .await
            .map_err(|error| PlatformError::Unsupported {
                capability: format!("等待 Linux 滚轮注入任务失败: {error}"),
            })?
    }

    #[cfg(not(target_os = "linux"))]
    {
        tokio::task::spawn_blocking(move || simulate_mouse_wheel(delta_y))
            .await
            .map_err(|error| PlatformError::Unsupported {
                capability: format!("等待滚轮注入任务失败: {error}"),
            })?
    }
}

/// 模拟鼠标垂直滚轮滚动。
///
/// `delta_y` 为正数表示向上滚动（远离用户），负数表示向下滚动（朝向用户）。
pub fn simulate_mouse_wheel(delta_y: f32) -> Result<(), PlatformError> {
    if delta_y.abs() < f32::EPSILON {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{mouse_event, MOUSEEVENTF_WHEEL};
        let clicks = (delta_y * 120.0) as i32;
        unsafe {
            mouse_event(MOUSEEVENTF_WHEEL, 0, 0, clicks, 0);
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        simulate_linux_wheel(delta_y)
    }

    #[cfg(target_os = "macos")]
    {
        simulate_macos_wheel(delta_y)
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = delta_y;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn simulate_macos_wheel(delta_y: f32) -> Result<(), PlatformError> {
    use core_graphics::event::{CGEvent, CGEventTapLocation, ScrollEventUnit};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|()| {
        PlatformError::Capture {
            message: "创建 macOS 滚轮事件源失败".to_string(),
        }
    })?;
    let lines = (delta_y * 3.0).round().clamp(-30.0, 30.0) as i32;
    let event =
        CGEvent::new_scroll_event(source, ScrollEventUnit::LINE, 1, lines, 0, 0).map_err(|()| {
            PlatformError::Capture {
                message: "创建 macOS 滚轮事件失败".to_string(),
            }
        })?;
    event.post(CGEventTapLocation::HID);
    Ok(())
}

#[cfg(target_os = "linux")]
fn simulate_linux_wheel(delta_y: f32) -> Result<(), PlatformError> {
    use std::process::Command;

    // Linux 滚轮通常映射为按键：4 为向上滚动，5 为向下滚动
    let button = if delta_y > 0.0 { "4" } else { "5" };

    // 优先尝试 ydotool (兼容 Wayland 与 X11)
    let ydo_btn = if delta_y > 0.0 { "0xC4" } else { "0xC5" };
    let mut ydotool = Command::new("ydotool");
    ydotool.args(["click", ydo_btn]);
    if output_with_timeout(ydotool, WHEEL_COMMAND_TIMEOUT)
        .is_ok_and(|output| output.status.success())
    {
        return Ok(());
    }

    // 降级尝试 xdotool (X11)
    let mut xdotool = Command::new("xdotool");
    xdotool.args(["click", button]);
    if output_with_timeout(xdotool, WHEEL_COMMAND_TIMEOUT)
        .is_ok_and(|output| output.status.success())
    {
        return Ok(());
    }

    Err(PlatformError::Unsupported {
        capability: "鼠标滚轮模拟（需要可用的 ydotool 或 xdotool）".to_string(),
    })
}
