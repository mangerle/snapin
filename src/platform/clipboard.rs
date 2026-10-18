//! 系统剪贴板读写集成。
//!
//! 支持在 Wayland 会话下读取和写入 RGBA 图像与 UTF-8 纯文本，
//! 供截图复制、放大镜拾色复制以及贴图按键读取复用。

use crate::error::SnapinError;
use arboard::{Clipboard, ImageData};
use image::RgbaImage;
use std::borrow::Cow;

/// 将 RGBA 图像写入系统剪贴板
pub fn write_image_to_clipboard(img: &RgbaImage) -> Result<(), SnapinError> {
    let mut clipboard = Clipboard::new().map_err(|e| SnapinError::ClipboardError {
        message: format!("初始化剪贴板失败: {e}"),
    })?;

    let image_data = ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: Cow::Borrowed(img.as_raw()),
    };

    clipboard
        .set_image(image_data)
        .map_err(|e| SnapinError::ClipboardError {
            message: format!("写入剪贴板图像失败: {e}"),
        })?;

    tracing::info!(
        "已成功将裁剪图像写入系统剪贴板，尺寸: {}x{}",
        img.width(),
        img.height()
    );
    Ok(())
}

/// 将纯文本字符串写入系统剪贴板
pub fn write_text_to_clipboard(text: &str) -> Result<(), SnapinError> {
    let mut clipboard = Clipboard::new().map_err(|e| SnapinError::ClipboardError {
        message: format!("初始化剪贴板失败: {e}"),
    })?;

    clipboard
        .set_text(text)
        .map_err(|e| SnapinError::ClipboardError {
            message: format!("写入剪贴板文本失败: {e}"),
        })?;

    tracing::info!("已将文本写入系统剪贴板: {text}");
    Ok(())
}

/// 从系统剪贴板读取纯文本内容 (若不存在则返回 None)
pub fn read_clipboard_text() -> Result<Option<String>, SnapinError> {
    let mut clipboard = Clipboard::new().map_err(|e| SnapinError::ClipboardError {
        message: format!("初始化剪贴板失败: {e}"),
    })?;

    match clipboard.get_text() {
        Ok(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        }
        Err(_) => Ok(None),
    }
}

/// 从系统剪贴板读取 RGBA 图像 (若不存在则返回 None)
pub fn read_clipboard_image() -> Result<Option<RgbaImage>, SnapinError> {
    let mut clipboard = Clipboard::new().map_err(|e| SnapinError::ClipboardError {
        message: format!("初始化剪贴板失败: {e}"),
    })?;

    match clipboard.get_image() {
        Ok(img_data) => {
            let width = img_data.width as u32;
            let height = img_data.height as u32;
            if let Some(rgba) = RgbaImage::from_raw(width, height, img_data.bytes.into_owned()) {
                Ok(Some(rgba))
            } else {
                Ok(None)
            }
        }
        Err(_) => Ok(None),
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
