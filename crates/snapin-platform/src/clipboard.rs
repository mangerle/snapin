//! 系统剪贴板读写集成。
//!
//! 支持在 Wayland 会话下读取和写入 RGBA 图像与 UTF-8 纯文本，
//! 供截图复制、放大镜拾色复制以及贴图按键读取复用。

use crate::error::PlatformError;
use arboard::{Clipboard, ImageData};
use image::RgbaImage;
use std::borrow::Cow;
use std::sync::Mutex;

static CLIPBOARD_INSTANCE: Mutex<Option<Clipboard>> = Mutex::new(None);
/// 单个剪贴板图像允许接收的最大像素数，防止异常内容长期占用数百兆内存。
const MAX_CLIPBOARD_PIXELS: u64 = 100_000_000;
/// 单条剪贴板文本允许接收的最大 UTF-8 字节数。
const MAX_CLIPBOARD_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// 在全局复用的剪贴板实例上执行操作闭包，具备断开连接时的单次自动重建自愈能力
fn with_clipboard<R, F: FnMut(&mut Clipboard) -> Result<R, arboard::Error>>(
    mut f: F,
) -> Result<R, PlatformError> {
    let mut guard = match CLIPBOARD_INSTANCE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("系统剪贴板句柄锁已中毒，销毁旧句柄后重建");
            let mut guard = poisoned.into_inner();
            *guard = None;
            guard
        }
    };

    if guard.is_none() {
        *guard = Clipboard::new().ok();
    }

    if let Some(ref mut cb) = *guard {
        match f(cb) {
            Ok(val) => Ok(val),
            Err(e) => {
                // 若底层句柄损坏，尝试重建一次剪贴板句柄自愈
                tracing::warn!("剪贴板操作失败，尝试重建连接句柄: {e}");
                #[cfg(target_os = "windows")]
                std::thread::sleep(std::time::Duration::from_millis(40));
                if let Ok(mut new_cb) = Clipboard::new() {
                    let res = f(&mut new_cb);
                    *guard = Some(new_cb);
                    res.map_err(|retry_err| PlatformError::Clipboard {
                        message: format!("剪贴板重试操作失败: {retry_err}"),
                    })
                } else {
                    Err(PlatformError::Clipboard {
                        message: format!("剪贴板操作失败: {e}"),
                    })
                }
            }
        }
    } else {
        Err(PlatformError::Clipboard {
            message: "无法初始化系统剪贴板".to_string(),
        })
    }
}

/// 将 RGBA 图像写入系统剪贴板
///
/// # Errors
///
/// 系统剪贴板不可用、图像或文本尺寸非法、像素长度不匹配或内容读写失败时返回 [`PlatformError`]。
pub fn write_image_to_clipboard(img: &RgbaImage) -> Result<(), PlatformError> {
    write_raw_image_to_clipboard(img.width(), img.height(), img.as_raw())
}

/// 将 RGBA 裸像素切片零拷贝写入系统剪贴板（避免包装临时 RgbaImage 与多余堆拷贝）
///
/// # Errors
///
/// 系统剪贴板不可用、图像或文本尺寸非法、像素长度不匹配或内容读写失败时返回 [`PlatformError`]。
pub fn write_raw_image_to_clipboard(
    width: u32,
    height: u32,
    raw_rgba: &[u8],
) -> Result<(), PlatformError> {
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| PlatformError::Clipboard {
            message: format!("剪贴板图像尺寸溢出，宽度: {width}，高度: {height}"),
        })?;
    if raw_rgba.len() != expected_len {
        return Err(PlatformError::Clipboard {
            message: format!(
                "剪贴板图像像素长度不匹配，期望 {expected_len} 字节，实际 {} 字节",
                raw_rgba.len()
            ),
        });
    }

    with_clipboard(|cb| {
        let image_data = ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Borrowed(raw_rgba),
        };
        cb.set_image(image_data)
    })?;

    tracing::info!(
        "已成功将像素数据零拷贝写入系统剪贴板，尺寸: {}x{}",
        width,
        height
    );
    Ok(())
}

/// 将纯文本字符串写入系统剪贴板
///
/// # Errors
///
/// 系统剪贴板不可用、图像或文本尺寸非法、像素长度不匹配或内容读写失败时返回 [`PlatformError`]。
pub fn write_text_to_clipboard(text: &str) -> Result<(), PlatformError> {
    with_clipboard(|cb| cb.set_text(text))?;

    tracing::debug!("已将文本写入系统剪贴板，字符数: {}", text.chars().count());
    Ok(())
}

/// 从系统剪贴板读取纯文本内容 (若不存在则返回 None)
///
/// # Errors
///
/// 系统剪贴板不可用、图像或文本尺寸非法、像素长度不匹配或内容读写失败时返回 [`PlatformError`]。
pub fn read_clipboard_text() -> Result<Option<String>, PlatformError> {
    let text = match with_clipboard(|cb| match cb.get_text() {
        Ok(text) => Ok(Some(text)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(error),
    }) {
        Ok(text) => text,
        Err(error) => {
            tracing::debug!("读取系统剪贴板文本失败，按无文本内容处理: {error}");
            return Ok(None);
        }
    };
    match text {
        Some(text) => validate_clipboard_text(text),
        None => Ok(None),
    }
}

fn validate_clipboard_text(text: String) -> Result<Option<String>, PlatformError> {
    if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
        Err(PlatformError::Clipboard {
            message: format!(
                "剪贴板文本超过安全上限，实际: {} 字节，上限: {MAX_CLIPBOARD_TEXT_BYTES} 字节",
                text.len()
            ),
        })
    } else if text.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}

/// 从系统剪贴板读取 RGBA 图像 (若不存在则返回 None)
///
/// # Errors
///
/// 系统剪贴板不可用、图像或文本尺寸非法、像素长度不匹配或内容读写失败时返回 [`PlatformError`]。
pub fn read_clipboard_image() -> Result<Option<RgbaImage>, PlatformError> {
    let image = match with_clipboard(|cb| match cb.get_image() {
        Ok(image) => Ok(Some(image)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(error),
    }) {
        Ok(image) => image,
        Err(error) => {
            tracing::debug!("读取系统剪贴板图像失败，按无图像内容处理: {error}");
            return Ok(None);
        }
    };
    image.map(clipboard_image_from_data).transpose()
}

fn clipboard_image_from_data(img_data: ImageData<'_>) -> Result<RgbaImage, PlatformError> {
    let width = u32::try_from(img_data.width).map_err(|_| PlatformError::Clipboard {
        message: format!("剪贴板图像宽度超出 u32 范围: {}", img_data.width),
    })?;
    let height = u32::try_from(img_data.height).map_err(|_| PlatformError::Clipboard {
        message: format!("剪贴板图像高度超出 u32 范围: {}", img_data.height),
    })?;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| PlatformError::Clipboard {
            message: format!("剪贴板图像尺寸溢出，宽度: {width}，高度: {height}"),
        })?;
    if pixels == 0 || pixels > MAX_CLIPBOARD_PIXELS {
        return Err(PlatformError::Clipboard {
            message: format!(
                "剪贴板图像像素数非法，尺寸: {width}x{height}，上限: {MAX_CLIPBOARD_PIXELS}"
            ),
        });
    }
    let expected_len = usize::try_from(pixels)
        .ok()
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| PlatformError::Clipboard {
            message: format!("剪贴板图像字节数超出平台范围，尺寸: {width}x{height}"),
        })?;
    let bytes = img_data.bytes.into_owned();
    if bytes.len() != expected_len {
        return Err(PlatformError::Clipboard {
            message: format!(
                "剪贴板图像像素长度不匹配，期望 {expected_len} 字节，实际 {} 字节",
                bytes.len()
            ),
        });
    }
    RgbaImage::from_raw(width, height, bytes).ok_or_else(|| PlatformError::Clipboard {
        message: format!("构建剪贴板图像失败，尺寸: {width}x{height}"),
    })
}

/// 剪贴板事件监听守卫，销毁时自动注销系统钩子与消息窗口。
pub struct ClipboardListenerGuard {
    #[cfg(target_os = "windows")]
    hwnd: windows_sys::Win32::Foundation::HWND,
}

unsafe impl Send for ClipboardListenerGuard {}
unsafe impl Sync for ClipboardListenerGuard {}

impl Drop for ClipboardListenerGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        if self.hwnd != 0 {
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_USER};
                PostMessageW(self.hwnd, WM_USER + 101, 0, 0);
            }
        }
    }
}

/// 订阅系统剪贴板更新事件（Windows 平台基于原生消息监听，非 Windows 平台降级为空通知器）。
pub fn subscribe_clipboard_changes() -> (ClipboardListenerGuard, tokio::sync::mpsc::Receiver<()>) {
    #[cfg(target_os = "windows")]
    {
        windows_listener::start_clipboard_listener()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        (ClipboardListenerGuard {}, rx)
    }
}

#[cfg(target_os = "windows")]
mod windows_listener {
    use super::ClipboardListenerGuard;
    use tokio::sync::mpsc::{channel, Receiver};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::DataExchange::{
        AddClipboardFormatListener, RemoveClipboardFormatListener,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        RegisterClassExW, TranslateMessage, HWND_MESSAGE, MSG, WM_CLIPBOARDUPDATE, WM_USER,
        WNDCLASSEXW,
    };

    const WM_STOP_LISTENER: u32 = WM_USER + 101;

    unsafe extern "system" fn listener_window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    pub(super) fn start_clipboard_listener() -> (ClipboardListenerGuard, Receiver<()>) {
        let (tx, rx) = channel(16);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);

        std::thread::spawn(move || unsafe {
            let class_name: Vec<u16> = "SnapinClipboardListener\0".encode_utf16().collect();
            let wnd_class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(listener_window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: 0,
                hIcon: 0,
                hCursor: 0,
                hbrBackground: 0,
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
                hIconSm: 0,
            };
            RegisterClassExW(&wnd_class);

            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                class_name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                0,
                0,
                std::ptr::null(),
            );

            if hwnd == 0 {
                let _ = ready_tx.send(0);
                return;
            }

            AddClipboardFormatListener(hwnd);
            let _ = ready_tx.send(hwnd);

            let mut msg = std::mem::zeroed::<MSG>();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                if msg.message == WM_STOP_LISTENER {
                    break;
                }
                if msg.message == WM_CLIPBOARDUPDATE {
                    let _ = tx.try_send(());
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            RemoveClipboardFormatListener(hwnd);
            DestroyWindow(hwnd);
        });

        let hwnd = ready_rx.recv().unwrap_or(0);
        (ClipboardListenerGuard { hwnd }, rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 裸像素长度与宽高必须严格匹配，避免把越界切片交给平台层。
    #[test]
    fn test_write_raw_image_rejects_length_mismatch() {
        let error = write_raw_image_to_clipboard(4, 4, &[0u8; 15]).expect_err("长度不匹配必须失败");
        assert!(matches!(error, PlatformError::Clipboard { .. }));
    }

    #[test]
    fn test_clipboard_basic() {
        // 在无显示服务的 headless 环境下 Clipboard::new 可能会返回 Err，
        // 此处验证函数能够安全返回 Result 而不触发 panic
        let _ = read_clipboard_text();
    }

    #[test]
    fn test_clipboard_image_data_rejects_length_mismatch() {
        let data = ImageData {
            width: 2,
            height: 2,
            bytes: Cow::Borrowed(&[0, 0, 0, 255]),
        };
        assert!(clipboard_image_from_data(data).is_err());
    }

    #[test]
    fn test_clipboard_image_data_rejects_oversized_dimensions() {
        let data = ImageData {
            width: 10_001,
            height: 10_000,
            bytes: Cow::Borrowed(&[]),
        };
        assert!(clipboard_image_from_data(data).is_err());
    }

    #[test]
    fn test_clipboard_text_validation_rejects_oversized_and_empty_values() {
        assert!(validate_clipboard_text("  ".to_string()).unwrap().is_none());
        assert!(validate_clipboard_text("x".repeat(MAX_CLIPBOARD_TEXT_BYTES + 1)).is_err());
        assert_eq!(
            validate_clipboard_text("有效文本".to_string())
                .unwrap()
                .as_deref(),
            Some("有效文本")
        );
    }
}
