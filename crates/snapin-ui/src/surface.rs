//! softbuffer 共享内存帧缓冲呈现与格式转换基础设施。

use crate::error::UiError;
use std::sync::Arc;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 将单个预乘 RGBA 像素打包为 softbuffer 所需的 `0xFFRRGGBB` 字。
#[inline]
fn pack_rgb(src: [u8; 4]) -> u32 {
    let rgba = u32::from_le_bytes(src);
    0xFF00_0000 | (rgba & 0x0000_FF00) | ((rgba & 0x0000_00FF) << 16) | ((rgba >> 16) & 0x0000_00FF)
}

/// 将 tiny-skia 像素复制到 softbuffer 共享内存缓冲区并提交显示。
///
/// # Errors
///
/// 缓冲区获取或提交失败返回 [`UiError::Softbuffer`]；绘图表面尺寸与像素源不一致时
/// 返回 [`UiError::Render`]。
pub fn blit_pixmap_to_surface(
    surface: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    pixmap: &Pixmap,
) -> Result<(), UiError> {
    let mut buffer = surface.buffer_mut().map_err(|error| UiError::Softbuffer {
        message: format!("获取绘图表面缓冲区失败: {error}"),
    })?;
    let src_chunks = pixmap.data().as_chunks::<4>().0;
    if src_chunks.len() != buffer.len() {
        return Err(UiError::Render {
            message: format!(
                "绘图表面像素容量与源位图不匹配，缓冲区: {}，源位图: {}",
                buffer.len(),
                src_chunks.len()
            ),
        });
    }
    for (dest, src) in buffer.iter_mut().zip(src_chunks) {
        *dest = pack_rgb(*src);
    }
    buffer.present().map_err(|error| UiError::Softbuffer {
        message: format!("帧缓冲提交显示失败: {error}"),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_rgb_forces_opaque_alpha() {
        assert_eq!(pack_rgb([10, 20, 30, 0]), 0xFF0A_141E);
        assert_eq!(pack_rgb([10, 20, 30, 255]), 0xFF0A_141E);
    }
}
