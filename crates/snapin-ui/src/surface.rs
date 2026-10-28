//! softbuffer 共享内存帧缓冲呈现与格式转换基础设施。
//!
//! 提供将 tiny-skia 的预乘 RGBA 像素高速 Blit 到操作系统原生 softbuffer ARGB8888 表面呈现的通用接口。

use crate::error::UiError;
use std::sync::Arc;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 将单个预乘 RGBA 像素打包为 softbuffer 所需的 `0x00RRGGBB` 字。
///
/// softbuffer 的无 alpha 表面只读取 RGB 三个通道，预乘后的 alpha 无需写入；
/// 使用 32 位字整读再位运算重排，比逐字节取移位少一次内存读且更利于编译器向量化。
#[inline]
fn pack_rgb(src: [u8; 4]) -> u32 {
    let rgba = u32::from_le_bytes(src);
    (rgba & 0x0000_FF00) | ((rgba & 0x0000_00FF) << 16) | ((rgba >> 16) & 0x0000_00FF)
}

/// 将整幅位图重排到 ARGB 字缓冲。
///
/// 目标缓冲必须与位图像素数量一致；长度不符时按较短一方截断，不越界写入。
pub fn swizzle_pixmap_full(pixmap: &Pixmap, words: &mut [u32]) {
    for (dest, src) in words.iter_mut().zip(pixmap.data().as_chunks::<4>().0) {
        *dest = pack_rgb(*src);
    }
}

/// 将位图中指定像素区域重排到 ARGB 字缓冲，区域按位图边界裁剪。
///
/// 增量重绘路径只更新光标覆盖层覆盖的局部区域，避免每帧对整屏数百万像素做字节重排。
pub fn swizzle_pixmap_region(pixmap: &Pixmap, words: &mut [u32], region: (u32, u32, u32, u32)) {
    let width = pixmap.width();
    let height = pixmap.height();
    let (rx, ry, rw, rh) = region;
    if rx >= width || ry >= height {
        return;
    }
    let rw = rw.min(width - rx) as usize;
    let rh = rh.min(height - ry) as usize;
    if rw == 0 || rh == 0 {
        return;
    }

    let stride = width as usize;
    let src = pixmap.data();
    for row in ry as usize..ry as usize + rh {
        let start = row * stride + rx as usize;
        let Some(src_row) = src.get(start * 4..(start + rw) * 4) else {
            return;
        };
        let Some(dest_row) = words.get_mut(start..start + rw) else {
            return;
        };
        for (dest, chunk) in dest_row.iter_mut().zip(src_row.as_chunks::<4>().0) {
            *dest = pack_rgb(*chunk);
        }
    }
}

/// 将 ARGB 字缓冲整体提交到 softbuffer 表面并呈现。
///
/// # Errors
///
/// 帧缓冲尺寸与字缓冲不一致时返回 [`UiError::Render`]，避免缩放/重绘竞态下静默残留旧帧；
/// 缓冲区获取或提交失败返回 [`UiError::Softbuffer`]。
pub fn blit_words_to_surface(
    surface: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    words: &[u32],
) -> Result<(), UiError> {
    let mut buffer = surface.buffer_mut().map_err(|e| UiError::Softbuffer {
        message: format!("获取绘图表面缓冲区失败: {e}"),
    })?;

    if words.len() != buffer.len() {
        return Err(UiError::Render {
            message: format!(
                "绘图表面像素容量与提交缓冲不匹配，缓冲区: {}，提交缓冲: {}",
                buffer.len(),
                words.len()
            ),
        });
    }

    buffer.copy_from_slice(words);
    buffer.present().map_err(|e| UiError::Softbuffer {
        message: format!("帧缓冲提交显示失败: {e}"),
    })?;

    Ok(())
}

/// 将 tiny-skia 像素高速复制到 softbuffer 共享内存缓冲区并提交显示。
///
/// # Errors
///
/// 缓冲区获取或提交失败返回 [`UiError::Softbuffer`]；绘图表面尺寸与像素源不一致时
/// 返回 [`UiError::Render`]，避免缩放/重绘竞态下静默残留上一帧像素。
pub fn blit_pixmap_to_surface(
    surface: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    pixmap: &Pixmap,
) -> Result<(), UiError> {
    let mut buffer = surface.buffer_mut().map_err(|e| UiError::Softbuffer {
        message: format!("获取绘图表面缓冲区失败: {e}"),
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

    buffer.present().map_err(|e| UiError::Softbuffer {
        message: format!("帧缓冲提交显示失败: {e}"),
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    /// 区域重排必须与整屏重排逐像素一致（含边界裁剪）
    #[test]
    fn test_swizzle_region_matches_full() {
        let mut pixmap = Pixmap::new(8, 6).expect("测试位图分配必须成功");
        pixmap.fill(Color::from_rgba8(10, 20, 30, 255));
        // 在区域外也写入不同像素，确保区域更新不会污染其它位置
        if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, 2.0, 2.0) {
            pixmap.fill_rect(
                rect,
                &{
                    let mut p = tiny_skia::Paint::default();
                    p.set_color_rgba8(200, 100, 50, 255);
                    p
                },
                tiny_skia::Transform::identity(),
                None,
            );
        }

        let mut full = vec![0u32; 8 * 6];
        swizzle_pixmap_full(&pixmap, &mut full);

        let mut partial = vec![0u32; 8 * 6];
        swizzle_pixmap_region(&pixmap, &mut partial, (3, 2, 4, 3));
        // 未覆盖区域保持初始值
        assert_eq!(partial[0], 0);
        // 覆盖区域与整屏结果一致
        for row in 2..5usize {
            for col in 3..7usize {
                let idx = row * 8 + col;
                assert_eq!(partial[idx], full[idx], "像素 ({col}, {row}) 不一致");
            }
        }

        // 区域越界时必须安全裁剪且不越界写入
        let mut clamped = vec![0u32; 8 * 6];
        swizzle_pixmap_region(&pixmap, &mut clamped, (6, 5, 100, 100));
        assert_eq!(clamped[5 * 8 + 6], full[5 * 8 + 6]);
        assert_eq!(clamped[5 * 8 + 7], full[5 * 8 + 7]);
    }
}
