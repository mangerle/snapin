//! 跨平台鼠标输入与滚轮模拟。
//!
//! # 设计原理
//!
//! 在长截图录制期间，截图全屏窗口位于顶层或处于指针穿透状态，需要将用户选区内的滚轮动作
//! 精准注入操作系统并投递到目标应用控件：
//! - **Linux GNOME 扩展**：通过 Mutter 合成器内虚拟指针按选区中心锚点聚焦目标窗口并双轴注入；
//! - **Linux 非 GNOME 降级**：优先使用支持双轴相对滚动的 `ydotool mousemove -w -- x y`，
//!   X11 环境回退至 `xdotool click --repeat`（并支持锚点定位与光标位置原子复原）；
//! - **Windows**：通过现代 `SendInput` 发送 `MOUSEINPUT`，保留亚-120 高分辨率滚轮增量
//!   并在提供 `focus_point` 时借助 RAII 守卫临时将光标定位于选区中心后立即归位；
//! - **macOS**：改用 `ScrollEventUnit::PIXEL` 派发平滑像素级滚动（避免粗粒度整行跳变），
//!   同样支持按主显示器缩放比换算的临时锚点与 RAII 光标复原。

use crate::error::PlatformError;

#[cfg(target_os = "linux")]
use crate::{
    capabilities::get_platform_capabilities, command::output_with_timeout,
    extension_client::simulate_mouse_wheel_extension,
};
#[cfg(target_os = "linux")]
use std::time::Duration;

/// 外部滚轮注入工具的最长等待时间，防止输入工具卡死长期占用阻塞线程。
#[cfg(target_os = "linux")]
const WHEEL_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

/// Win32 单格标准滚轮常量（`WHEEL_DELTA`）。
#[cfg(any(target_os = "windows", test))]
const WIN_WHEEL_DELTA: f64 = 120.0;
/// macOS 每个逻辑滚动行对应的平滑像素换算系数。
#[cfg(any(target_os = "macos", test))]
const MACOS_PIXELS_PER_LINE: f64 = 36.0;
/// macOS 单次滚动事件允许注入的最大像素步长，防止快速甩动直接越过相邻帧重叠区。
#[cfg(any(target_os = "macos", test))]
const MACOS_MAX_PIXEL_STEP: f64 = 360.0;
/// Linux CLI 回退工具单次允许注入的最大重复步数。
#[cfg(any(target_os = "linux", test))]
const LINUX_MAX_WHEEL_STEPS: i32 = 20;

/// 二维滚轮模拟请求参数。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WheelInputSpec {
    /// 水平滚轮增量（正数向左、负数向右）
    pub delta_x: f32,
    /// 垂直滚轮增量（正数向上、负数向下）
    pub delta_y: f32,
    /// 目标窗口所在选区中心的物理像素坐标，供合成器或光标临时锚点精准投递。
    pub focus_point: Option<(f32, f32)>,
}

/// 异步模拟鼠标二维滚轮滚动（支持垂直与水平长截图）。
///
/// # Errors
///
/// 当前平台没有可用滚轮注入通路、扩展调用失败，或阻塞任务异常结束时返回 [`PlatformError`]。
pub async fn simulate_mouse_wheel_async(delta_x: f32, delta_y: f32) -> Result<(), PlatformError> {
    simulate_mouse_wheel_with_focus_async(WheelInputSpec {
        delta_x,
        delta_y,
        focus_point: None,
    })
    .await
}

/// 带目标窗口聚焦锚点的异步二维滚轮模拟入口。
///
/// # Errors
///
/// 当前平台没有可用滚轮注入通路、扩展调用失败，或阻塞任务异常结束时返回 [`PlatformError`]。
pub async fn simulate_mouse_wheel_with_focus_async(
    spec: WheelInputSpec,
) -> Result<(), PlatformError> {
    if !has_effective_wheel_delta(spec.delta_x, spec.delta_y) {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        if get_platform_capabilities().gnome_extension_active {
            match simulate_mouse_wheel_extension(spec.delta_x, spec.delta_y, spec.focus_point).await
            {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    tracing::debug!("GNOME 扩展拒绝注入滚轮事件，降级到外部输入工具");
                }
                Err(error) => {
                    tracing::debug!("GNOME 扩展滚轮注入失败，降级到外部输入工具: {error}");
                }
            }
        }
        tokio::task::spawn_blocking(move || simulate_linux_wheel(spec))
            .await
            .map_err(|error| PlatformError::Unsupported {
                capability: format!("等待 Linux 滚轮注入任务失败: {error}"),
            })?
    }

    #[cfg(not(target_os = "linux"))]
    {
        tokio::task::spawn_blocking(move || simulate_wheel_sync(spec))
            .await
            .map_err(|error| PlatformError::Unsupported {
                capability: format!("等待滚轮注入任务失败: {error}"),
            })?
    }
}

/// 模拟鼠标二维滚轮滚动（同步阻塞入口）。
///
/// `delta_x` 正数向左、负数向右；`delta_y` 正数向上、负数向下。
///
/// # Errors
///
/// 当前平台缺少可用的滚轮注入通路、外部输入工具执行失败，或原生事件源创建失败时
/// 返回 [`PlatformError`]；零位移会直接成功返回，不产生任何输入事件。
pub fn simulate_mouse_wheel(delta_x: f32, delta_y: f32) -> Result<(), PlatformError> {
    if !has_effective_wheel_delta(delta_x, delta_y) {
        return Ok(());
    }
    simulate_wheel_sync(WheelInputSpec {
        delta_x,
        delta_y,
        focus_point: None,
    })
}

fn simulate_wheel_sync(spec: WheelInputSpec) -> Result<(), PlatformError> {
    #[cfg(target_os = "windows")]
    {
        simulate_windows_wheel(spec)
    }

    #[cfg(target_os = "linux")]
    {
        simulate_linux_wheel(spec)
    }

    #[cfg(target_os = "macos")]
    {
        simulate_macos_wheel(spec)
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = spec;
        Ok(())
    }
}

/// 将任意非零浮点滚动行数无损缩放为 Win32 `mouseData`（支持小于 `WHEEL_DELTA(120)` 的高精度滚动）。
#[cfg(any(target_os = "windows", test))]
#[must_use]
pub(crate) fn compute_windows_wheel_dw_data(delta: f32) -> i32 {
    if !is_nonzero_delta(delta) {
        return 0;
    }
    let scaled = (f64::from(delta) * WIN_WHEEL_DELTA).round();
    let max = f64::from(i32::MAX);
    let clamped = scaled.clamp(-max, max) as i32;
    if clamped == 0 {
        if delta.is_sign_positive() {
            1
        } else {
            -1
        }
    } else {
        clamped
    }
}

/// 将任意非零浮点滚动增量换算为 macOS `ScrollEventUnit::PIXEL` 平滑像素偏移，最低保留 1 像素。
#[cfg(any(target_os = "macos", test))]
#[must_use]
pub(crate) fn compute_macos_pixel_scroll(delta: f32) -> i32 {
    if !is_nonzero_delta(delta) {
        return 0;
    }
    let rounded = (f64::from(delta) * MACOS_PIXELS_PER_LINE)
        .round()
        .clamp(-MACOS_MAX_PIXEL_STEP, MACOS_MAX_PIXEL_STEP) as i32;
    if rounded == 0 {
        if delta.is_sign_positive() {
            1
        } else {
            -1
        }
    } else {
        rounded
    }
}

/// 将任意非零浮点滚动增量换算为 Linux 回退工具的离散带符号步数（1..=20 步）。
#[cfg(any(target_os = "linux", test))]
#[must_use]
pub(crate) fn compute_linux_wheel_steps(delta: f32) -> i32 {
    if !is_nonzero_delta(delta) {
        return 0;
    }
    let magnitude = (f64::from(delta).abs().round() as i32).clamp(1, LINUX_MAX_WHEEL_STEPS);
    if delta.is_sign_positive() {
        magnitude
    } else {
        -magnitude
    }
}

#[cfg(target_os = "windows")]
struct WindowsCursorRestoreGuard(Option<windows_sys::Win32::Foundation::POINT>);

#[cfg(target_os = "windows")]
impl Drop for WindowsCursorRestoreGuard {
    fn drop(&mut self) {
        if let Some(prev) = self.0 {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos(prev.x, prev.y);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn simulate_windows_wheel(spec: WheelInputSpec) -> Result<(), PlatformError> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetSystemMetrics, SetCursorPos, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    let _cursor_guard = spec.focus_point.and_then(|(fx, fy)| {
        let mut prev = POINT::default();
        let captured = unsafe { GetCursorPos(&mut prev) != 0 };
        let origin_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let origin_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let target_x = origin_x.saturating_add(fx.round() as i32);
        let target_y = origin_y.saturating_add(fy.round() as i32);
        if captured && (prev.x != target_x || prev.y != target_y) {
            unsafe { SetCursorPos(target_x, target_y) };
            Some(WindowsCursorRestoreGuard(Some(prev)))
        } else {
            None
        }
    });

    let mut inputs = [INPUT::default(); 2];
    let mut count = 0usize;
    let wheel_y = compute_windows_wheel_dw_data(spec.delta_y);
    if wheel_y != 0 {
        inputs[count] = build_win_wheel_input(MOUSEEVENTF_WHEEL, wheel_y as u32);
        count += 1;
    }
    // Win32 WM_MOUSEHWHEEL 正值表示向右滚动，与 Winit delta_x 符号相反。
    let wheel_x = compute_windows_wheel_dw_data(-spec.delta_x);
    if wheel_x != 0 {
        inputs[count] = build_win_wheel_input(MOUSEEVENTF_HWHEEL, wheel_x as u32);
        count += 1;
    }
    if count > 0 {
        let sent = unsafe {
            SendInput(
                count as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if sent == 0 {
            return Err(PlatformError::Unsupported {
                capability: "Windows SendInput 滚轮注入失败".to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn build_win_wheel_input(
    dw_flags: u32,
    mouse_data: u32,
) -> windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT,
    };
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: mouse_data,
                dwFlags: dw_flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(target_os = "macos")]
struct MacosCursorRestoreGuard(Option<core_graphics::geometry::CGPoint>);

#[cfg(target_os = "macos")]
impl Drop for MacosCursorRestoreGuard {
    fn drop(&mut self) {
        if let Some(prev) = self.0 {
            let _ = core_graphics::display::CGDisplay::warp_mouse_cursor_position(prev);
        }
    }
}

#[cfg(target_os = "macos")]
fn simulate_macos_wheel(spec: WheelInputSpec) -> Result<(), PlatformError> {
    use core_graphics::display::CGDisplay;
    use core_graphics::event::{CGEvent, CGEventTapLocation, ScrollEventUnit};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|()| {
        PlatformError::Capture {
            message: "创建 macOS 滚轮事件源失败".to_string(),
        }
    })?;
    let _cursor_guard = spec.focus_point.and_then(|(fx, fy)| {
        let prev = CGEvent::new(source.clone()).ok().map(|ev| ev.location())?;
        let main = CGDisplay::main();
        let bounds = main.bounds();
        let scale = if bounds.size.width > 0.0 {
            (main.pixels_wide() as f64 / bounds.size.width).max(1.0)
        } else {
            1.0
        };
        let target = CGPoint::new(
            bounds.origin.x + f64::from(fx) / scale,
            bounds.origin.y + f64::from(fy) / scale,
        );
        CGDisplay::warp_mouse_cursor_position(target).ok()?;
        Some(MacosCursorRestoreGuard(Some(prev)))
    });

    let pixels_y = compute_macos_pixel_scroll(spec.delta_y);
    let pixels_x = compute_macos_pixel_scroll(spec.delta_x);
    let event = CGEvent::new_scroll_event(source, ScrollEventUnit::PIXEL, 2, pixels_y, pixels_x, 0)
        .map_err(|()| PlatformError::Capture {
            message: "创建 macOS 像素级滚轮事件失败".to_string(),
        })?;
    event.post(CGEventTapLocation::HID);
    Ok(())
}

#[cfg(target_os = "linux")]
fn simulate_linux_wheel(spec: WheelInputSpec) -> Result<(), PlatformError> {
    let steps_y = compute_linux_wheel_steps(spec.delta_y);
    let steps_x = compute_linux_wheel_steps(spec.delta_x);
    if steps_x == 0 && steps_y == 0 {
        return Ok(());
    }
    if try_ydotool_wheel(steps_x, steps_y) {
        return Ok(());
    }
    if try_xdotool_wheel(steps_x, steps_y, spec.focus_point) {
        return Ok(());
    }
    Err(PlatformError::Unsupported {
        capability: "鼠标滚轮模拟（需要可用的 ydotool 1.x 或 xdotool）".to_string(),
    })
}

#[cfg(target_os = "linux")]
fn try_ydotool_wheel(steps_x: i32, steps_y: i32) -> bool {
    use std::process::Command;
    // Linux evdev REL_HWHEEL 正值向右，与 Winit delta_x 方向相反。
    let evdev_x = (-steps_x).to_string();
    let evdev_y = steps_y.to_string();
    let mut cmd = Command::new("ydotool");
    cmd.args(["mousemove", "-w", "--", &evdev_x, &evdev_y]);
    output_with_timeout(cmd, WHEEL_COMMAND_TIMEOUT).is_ok_and(|output| output.status.success())
}

#[cfg(target_os = "linux")]
fn try_xdotool_wheel(steps_x: i32, steps_y: i32, focus_point: Option<(f32, f32)>) -> bool {
    use std::process::Command;

    let mut cmd = Command::new("xdotool");
    if let Some((fx, fy)) = focus_point {
        cmd.args([
            "mousemove",
            "--sync",
            &fx.round().to_string(),
            &fy.round().to_string(),
        ]);
    }
    if steps_y != 0 {
        let btn = if steps_y > 0 { "4" } else { "5" };
        cmd.args(["click", "--repeat", &steps_y.abs().to_string(), btn]);
    }
    if steps_x != 0 {
        let btn = if steps_x > 0 { "6" } else { "7" };
        cmd.args(["click", "--repeat", &steps_x.abs().to_string(), btn]);
    }
    if focus_point.is_some() {
        cmd.args(["mousemove", "restore"]);
    }
    output_with_timeout(cmd, WHEEL_COMMAND_TIMEOUT).is_ok_and(|output| output.status.success())
}

fn is_nonzero_delta(delta: f32) -> bool {
    delta.is_finite() && delta.abs() >= f32::EPSILON
}

fn has_effective_wheel_delta(delta_x: f32, delta_y: f32) -> bool {
    is_nonzero_delta(delta_x) || is_nonzero_delta(delta_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_wheel_preserves_sub_120_and_rounding() {
        assert_eq!(compute_windows_wheel_dw_data(0.0), 0);
        assert_eq!(compute_windows_wheel_dw_data(f32::NAN), 0);
        assert_eq!(compute_windows_wheel_dw_data(-1.0), -120);
        assert_eq!(compute_windows_wheel_dw_data(0.5), 60);
        // 极微小非零滚动绝不能截断为 0 丢弃
        assert_eq!(compute_windows_wheel_dw_data(0.002), 1);
        assert_eq!(compute_windows_wheel_dw_data(-0.002), -1);
    }

    #[test]
    fn test_macos_pixel_scroll_preserves_fractional_precision() {
        assert_eq!(compute_macos_pixel_scroll(0.0), 0);
        assert_eq!(compute_macos_pixel_scroll(-0.25), -9);
        assert_eq!(compute_macos_pixel_scroll(0.01), 1);
        assert_eq!(compute_macos_pixel_scroll(-50.0), -360);
    }

    #[test]
    fn test_linux_wheel_steps_scale_proportionally_and_clamp() {
        assert_eq!(compute_linux_wheel_steps(0.0), 0);
        assert_eq!(compute_linux_wheel_steps(-0.2), -1);
        assert_eq!(compute_linux_wheel_steps(3.4), 3);
        assert_eq!(compute_linux_wheel_steps(-100.0), -20);
    }
}
