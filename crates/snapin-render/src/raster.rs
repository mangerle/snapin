//! 字形像素预乘 Alpha 混合的底层光栅化原语。

/// 待混合的字形 RGBA 像素。
#[derive(Debug, Clone, Copy)]
pub(super) struct GlyphPixel {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

/// 将单个字形像素按预乘 Alpha 混合写入目标 RGBA 缓冲区。
///
/// tiny-skia 的 `Pixmap` 缓冲区使用预乘 Alpha 布局，因此目标通道值已经是
/// “颜色 × 自身 Alpha”的结果，混合时只能再乘 `(1 - src_a)`，绝不能再乘一次
/// `dst_a`，也不能除以 `out_a` 转回直通色。违反该约定会让透明贴图与磨砂玻璃
/// 擦除层上的文字偏亮，并污染后续按预乘假设采样的模糊图元。
#[inline]
pub(super) fn blend_glyph_rgba(data: &mut [u8], idx: usize, pixel: GlyphPixel) {
    let src_a = pixel.alpha as f32 / 255.0;
    let dst_a = data[idx + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        return;
    }

    let inv_src_a = 1.0 - src_a;
    data[idx] = (pixel.red as f32 * src_a + data[idx] as f32 * inv_src_a).round() as u8;
    data[idx + 1] = (pixel.green as f32 * src_a + data[idx + 1] as f32 * inv_src_a).round() as u8;
    data[idx + 2] = (pixel.blue as f32 * src_a + data[idx + 2] as f32 * inv_src_a).round() as u8;
    data[idx + 3] = (out_a * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 半透明目标像素必须按预乘 Alpha 混合：目标颜色只乘 `(1 - src_a)`，
    /// 不能再次乘 `dst_a`，也不能除以 `out_a`。
    #[test]
    fn test_blend_glyph_keeps_premultiplied_destination() {
        // 目标为预乘红色 (64, 0, 0, 128)，源为半透明蓝色 (0, 0, 255, 128)
        let mut data = [64u8, 0, 0, 128];
        blend_glyph_rgba(
            &mut data,
            0,
            GlyphPixel {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 128,
            },
        );

        // out_a = 0.502 + 0.502 * 0.498 = 0.752 -> 192
        // out_r = 64 * 0.498 = 32，out_b = 255 * 0.502 = 128
        assert_eq!(data, [32, 0, 128, 192]);
    }
}
