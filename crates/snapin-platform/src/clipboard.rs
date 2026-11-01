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
    write_raw_image_to_clipboard(img.width(), img.height(), img.as_raw())
}

/// 将 RGBA 裸像素切片零拷贝写入系统剪贴板（避免包装临时 RgbaImage 与多余堆拷贝）
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
pub fn write_text_to_clipboard(text: &str) -> Result<(), PlatformError> {
    with_clipboard(|cb| cb.set_text(text))?;

    tracing::debug!("已将文本写入系统剪贴板，字符数: {}", text.chars().count());
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
            let width = u32::try_from(img_data.width).map_err(|_| arboard::Error::Unknown {
                description: format!("剪贴板图像宽度超出 u32 范围: {}", img_data.width),
            })?;
            let height = u32::try_from(img_data.height).map_err(|_| arboard::Error::Unknown {
                description: format!("剪贴板图像高度超出 u32 范围: {}", img_data.height),
            })?;
            let expected_len = (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or_else(|| arboard::Error::Unknown {
                    description: format!("剪贴板图像尺寸溢出，宽度: {width}，高度: {height}"),
                })?;
            let bytes = img_data.bytes.into_owned();
            if bytes.len() != expected_len {
                return Err(arboard::Error::Unknown {
                    description: format!(
                        "剪贴板图像像素长度不匹配，期望 {expected_len} 字节，实际 {} 字节",
                        bytes.len()
                    ),
                });
            }
            if let Some(rgba) = RgbaImage::from_raw(width, height, bytes) {
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
}
