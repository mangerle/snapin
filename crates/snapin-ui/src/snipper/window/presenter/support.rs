//! 截图全屏窗口呈现辅助工具：管理底层表面、底图生成与放大镜签名。

use std::num::NonZeroU32;
use std::sync::Arc;

use image::RgbaImage;
use tiny_skia::Pixmap;
use winit::window::Window;

use super::MagnifierSignature;
use crate::error::UiError;
use crate::snipper::magnifier::Magnifier;

/// 使用定点乘法快速生成截图全屏暗化蒙层。
///
/// # Errors
///
/// 像素缓冲分配失败时返回 [`UiError`]。
pub fn create_dimmed_pixmap_fast(img: &RgbaImage) -> Result<Pixmap, UiError> {
    let w = img.width();
    let h = img.height();
    let mut dimmed = Pixmap::new(w, h).ok_or(UiError::PixmapAlloc {
        width: w,
        height: h,
    })?;

    let src = img.as_raw();
    let dst_dim = dimmed.data_mut();

    // 内存单遍流转：仅填充定点整数暗化蒙层
    for (d_dim, s) in dst_dim
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(src.as_chunks::<4>().0)
    {
        d_dim[0] = ((s[0] as u16 * 107) >> 8) as u8;
        d_dim[1] = ((s[1] as u16 * 107) >> 8) as u8;
        d_dim[2] = ((s[2] as u16 * 107) >> 8) as u8;
        d_dim[3] = s[3];
    }

    Ok(dimmed)
}

/// 创建并调整 softbuffer 表面到指定非零尺寸。
pub(super) fn create_softbuffer_surface(
    window: Arc<Window>,
    width: u32,
    height: u32,
) -> Result<softbuffer::Surface<Arc<Window>, Arc<Window>>, UiError> {
    let context =
        softbuffer::Context::new(window.clone()).map_err(|error| UiError::Softbuffer {
            message: error.to_string(),
        })?;
    let mut surface =
        softbuffer::Surface::new(&context, window).map_err(|error| UiError::Softbuffer {
            message: error.to_string(),
        })?;
    let width = NonZeroU32::new(width).ok_or_else(|| UiError::Softbuffer {
        message: "无效的屏幕宽度".to_string(),
    })?;
    let height = NonZeroU32::new(height).ok_or_else(|| UiError::Softbuffer {
        message: "无效的屏幕高度".to_string(),
    })?;
    surface
        .resize(width, height)
        .map_err(|error| UiError::Softbuffer {
            message: format!("初始化帧缓冲尺寸失败: {error}"),
        })?;
    Ok(surface)
}

/// 根据截图模式创建原始底图或预计算暗化底图。
pub(super) fn create_base_pixmap(
    bg_image: &RgbaImage,
    width: u32,
    height: u32,
    dimmed: bool,
) -> Result<Pixmap, UiError> {
    if dimmed {
        return create_dimmed_pixmap_fast(bg_image);
    }
    let mut pixmap = Pixmap::new(width, height).ok_or(UiError::PixmapAlloc { width, height })?;
    pixmap.data_mut().copy_from_slice(bg_image.as_raw());
    Ok(pixmap)
}

/// 预分配与画布像素一一对应的提交缓冲。
pub(super) fn create_frame_words(width: u32, height: u32) -> Result<Vec<u32>, UiError> {
    let pixel_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or(UiError::PixmapAlloc { width, height })?;
    let mut frame_words = Vec::new();
    frame_words
        .try_reserve_exact(pixel_count)
        .map_err(|_| UiError::PixmapAlloc { width, height })?;
    frame_words.resize(pixel_count, 0);
    Ok(frame_words)
}

/// 生成放大镜配置签名，用于判断卡片几何或外观是否发生变化。
pub(super) fn magnifier_signature(magnifier: &Magnifier) -> MagnifierSignature {
    (
        magnifier.enabled,
        magnifier.scale,
        magnifier.show_grid,
        magnifier.ui_scale,
    )
}
