//! 字形与位图像素预乘 Alpha 混合与双向格式转换原语。

use image::RgbaImage;
use tiny_skia::{ColorU8, Pixmap, PixmapMut, PremultipliedColorU8};

/// 待混合的字形 RGBA 像素。
#[derive(Debug, Clone, Copy)]
pub(super) struct GlyphPixel {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

/// 从直通 RGBA 图像向预乘 Alpha `Pixmap` 拷贝的子区域描述。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlitRgbaRegion {
    /// 源图像左上角 X 坐标
    pub src_x: u32,
    /// 源图像左上角 Y 坐标
    pub src_y: u32,
    /// 目标 Pixmap 左上角 X 坐标
    pub dst_x: u32,
    /// 目标 Pixmap 左上角 Y 坐标
    pub dst_y: u32,
    /// 拷贝区域宽度（像素）
    pub width: u32,
    /// 拷贝区域高度（像素）
    pub height: u32,
}

/// 将单个直通（Unpremultiplied）RGBA 像素转换为 tiny-skia 预乘格式。
#[inline]
pub fn premultiply_rgba_pixel(rgba: [u8; 4]) -> [u8; 4] {
    let [r, g, b, a] = rgba;
    if a == 255 {
        return rgba;
    }
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let premul = ColorU8::from_rgba(r, g, b, a).premultiply();
    [premul.red(), premul.green(), premul.blue(), premul.alpha()]
}

/// 将单个 tiny-skia 预乘 RGBA 像素还原为直通（Unpremultiplied）格式。
#[inline]
pub fn demultiply_rgba_pixel(rgba: [u8; 4]) -> [u8; 4] {
    let [r, g, b, a] = rgba;
    if a == 255 {
        return rgba;
    }
    if a == 0 {
        return [0, 0, 0, 0];
    }
    // 防御局部后处理算法写出略超 alpha 的非法通道，确保恢复过程永不失败。
    PremultipliedColorU8::from_rgba(r.min(a), g.min(a), b.min(a), a).map_or([0, 0, 0, 0], |p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    })
}

/// 将一行直通 RGBA 像素写入预乘目标行；整行完全不透明时直接走内存块拷贝快路径。
#[inline]
fn write_premultiplied_row(src_row: &[u8], dst_row: &mut [u8]) {
    if src_row.as_chunks::<4>().0.iter().all(|px| px[3] == 255) {
        dst_row.copy_from_slice(src_row);
        return;
    }
    for (dst, src) in dst_row
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(src_row.as_chunks::<4>().0.iter())
    {
        *dst = premultiply_rgba_pixel(*src);
    }
}

/// 将直通 RGBA `RgbaImage` 转换为 tiny-skia 预乘 `Pixmap`。
///
/// 实现初衷：`image::RgbaImage` 始终采用直通 Alpha 语义，而 `tiny_skia::Pixmap`
/// 假定缓冲区已预乘。若直接整块 `copy_from_slice` 传入半透明或 Alpha=0 像素，
/// 会导致混合计算饱和溢出并留下黑边/紫边。
pub fn rgba_image_to_pixmap(image: &RgbaImage) -> Option<Pixmap> {
    let (w, h) = image.dimensions();
    let mut pixmap = Pixmap::new(w, h)?;
    write_premultiplied_row(image.as_raw(), pixmap.data_mut());
    Some(pixmap)
}

/// 将已渲染完毕的 tiny-skia `Pixmap` 消费并反预乘为直通 `RgbaImage`。
///
/// 实现初衷：导出 PNG、写入剪贴板或交由下一轮贴图美化前，必须将半透明合成结果
/// 从预乘空间还原回直通 RGBA，避免重复预乘造成阴影与半透明边缘发黑。
/// 当全部像素不透明时零开销原地复用缓冲，仅对 `0 < alpha < 255` 像素做反预乘。
pub fn pixmap_into_rgba_image(pixmap: Pixmap) -> Option<RgbaImage> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut raw = pixmap.take();
    for px in raw.as_chunks_mut::<4>().0.iter_mut() {
        if px[3] != 255 {
            *px = demultiply_rgba_pixel(*px);
        }
    }
    RgbaImage::from_raw(w, h, raw)
}

/// 将直通 RGBA 源图的指定子区域预乘并拷贝到目标 `PixmapMut` 指定坐标。
///
/// 用于长图视口与缩略图直接切片上屏，避免中间分配 `view.to_image()` 堆缓冲，
/// 同时严格维持预乘像素布局契约。
pub fn blit_straight_rgba_region_to_pixmap(
    src: &RgbaImage,
    dst: &mut PixmapMut<'_>,
    region: BlitRgbaRegion,
) {
    let (sw, sh) = src.dimensions();
    let (dw, dh) = (dst.width(), dst.height());
    let copy_w = region
        .width
        .min(sw.saturating_sub(region.src_x))
        .min(dw.saturating_sub(region.dst_x));
    let copy_h = region
        .height
        .min(sh.saturating_sub(region.src_y))
        .min(dh.saturating_sub(region.dst_y));
    if copy_w == 0 || copy_h == 0 {
        return;
    }

    let src_raw = src.as_raw();
    let dst_raw = dst.data_mut();
    let src_stride = sw as usize * 4;
    let dst_stride = dw as usize * 4;
    let row_bytes = copy_w as usize * 4;

    for row in 0..copy_h as usize {
        let s_start = (region.src_y as usize + row) * src_stride + region.src_x as usize * 4;
        let d_start = (region.dst_y as usize + row) * dst_stride + region.dst_x as usize * 4;
        if let (Some(src_slice), Some(dst_slice)) = (
            src_raw.get(s_start..s_start + row_bytes),
            dst_raw.get_mut(d_start..d_start + row_bytes),
        ) {
            write_premultiplied_row(src_slice, dst_slice);
        }
    }
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
    use image::Rgba;

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

    /// 直通 RGBA 与 tiny-skia 预乘 Pixmap 的往返转换必须正确处理半透明与全透明像素。
    #[test]
    fn test_straight_and_premultiplied_roundtrip() {
        let mut img = RgbaImage::new(3, 1);
        img.put_pixel(0, 0, Rgba([200, 100, 50, 255]));
        img.put_pixel(1, 0, Rgba([255, 0, 0, 128]));
        img.put_pixel(2, 0, Rgba([255, 120, 80, 0]));

        let pixmap = rgba_image_to_pixmap(&img).expect("应能构造 Pixmap");
        assert_eq!(
            &pixmap.data()[0..4],
            &[200, 100, 50, 255],
            "不透明像素应保持不变"
        );
        assert_eq!(
            &pixmap.data()[4..8],
            &[128, 0, 0, 128],
            "半透明红色应预乘为 128"
        );
        assert_eq!(
            &pixmap.data()[8..12],
            &[0, 0, 0, 0],
            "全透明像素色彩通道必须归零"
        );

        let restored = pixmap_into_rgba_image(pixmap).expect("应能转回 RgbaImage");
        assert_eq!(restored.get_pixel(0, 0).0, [200, 100, 50, 255]);
        assert_eq!(restored.get_pixel(1, 0).0, [255, 0, 0, 128]);
        assert_eq!(restored.get_pixel(2, 0).0, [0, 0, 0, 0]);
    }

    /// 区域 blit 必须按目标与源尺寸安全裁切并预乘写入目标位图。
    #[test]
    fn test_blit_straight_rgba_region_premultiplies_and_clips() {
        let mut src = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));
        src.put_pixel(2, 1, Rgba([200, 100, 0, 128]));

        let mut dst = Pixmap::new(3, 3).expect("目标 Pixmap 应创建成功");
        blit_straight_rgba_region_to_pixmap(
            &src,
            &mut dst.as_mut(),
            BlitRgbaRegion {
                src_x: 1,
                src_y: 1,
                dst_x: 1,
                dst_y: 0,
                width: 4,
                height: 4,
            },
        );

        // src(2,1) 落在 dst(2,0)
        let idx = 2 * 4;
        assert_eq!(&dst.data()[idx..idx + 4], &[100, 50, 0, 128]);
    }
}
