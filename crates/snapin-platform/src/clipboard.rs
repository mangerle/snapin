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

/// 在全局复用的剪贴板实例上执行操作闭包，具备断开连接时的单次自动重建自愈能力
fn with_clipboard<R, F: FnMut(&mut Clipboard) -> Result<R, arboard::Error>>(
    mut f: F,
) -> Result<R, PlatformError> {
    let mut guard = CLIPBOARD_INSTANCE
        .lock()
        .map_err(|e| PlatformError::Clipboard {
            message: format!("剪贴板互斥锁中毒: {e}"),
        })?;

    if guard.is_none() {
        *guard = Clipboard::new().ok();
    }

    if let Some(ref mut cb) = *guard {
        match f(cb) {
            Ok(val) => Ok(val),
            Err(e) => {
                // 若底层句柄损坏，尝试重建一次剪贴板句柄自愈
                tracing::warn!("剪贴板操作失败，尝试重建连接句柄: {e}");
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
pub fn write_image_to_clipboard(img: &RgbaImage) -> Result<(), PlatformError> {
    with_clipboard(|cb| {
        let image_data = ImageData {
            width: img.width() as usize,
            height: img.height() as usize,
            bytes: Cow::Borrowed(img.as_raw()),
        };
        cb.set_image(image_data)
    })?;

    tracing::info!(
        "已成功将裁剪图像写入系统剪贴板，尺寸: {}x{}",
        img.width(),
        img.height()
    );
    Ok(())
}

/// 将纯文本字符串写入系统剪贴板
pub fn write_text_to_clipboard(text: &str) -> Result<(), PlatformError> {
    with_clipboard(|cb| cb.set_text(text))?;

    tracing::info!("已将文本写入系统剪贴板: {text}");
    Ok(())
}

/// 从系统剪贴板读取纯文本内容 (若不存在则返回 None)
pub fn read_clipboard_text() -> Result<Option<String>, PlatformError> {
    let res = with_clipboard(|cb| match cb.get_text() {
        Ok(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        }
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(e) => Err(e),
    });

    match res {
        Ok(opt) => Ok(opt),
        Err(PlatformError::Clipboard { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// 从系统剪贴板读取 RGBA 图像 (若不存在则返回 None)
pub fn read_clipboard_image() -> Result<Option<RgbaImage>, PlatformError> {
    let res = with_clipboard(|cb| match cb.get_image() {
        Ok(img_data) => {
            let width = img_data.width as u32;
            let height = img_data.height as u32;
            if let Some(rgba) = RgbaImage::from_raw(width, height, img_data.bytes.into_owned()) {
                Ok(Some(rgba))
            } else {
                Ok(None)
            }
        }
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(e) => Err(e),
    });

    match res {
        Ok(opt) => Ok(opt),
        Err(PlatformError::Clipboard { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_basic() {
        // 在无显示服务的 headless 环境下 Clipboard::new 可能会返回 Err，
        // 此处验证函数能够安全返回 Result 而不触发 panic
        let _ = read_clipboard_text();
    }
}
