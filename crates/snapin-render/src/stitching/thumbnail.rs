//! 长截图侧边实时缩略图生成与极端长宽比自适应窗口裁剪算法。
//!
//! # 设计原理
//!
//! 传统整图等比缩放面对 `800×40000` 这类超长网页或日志长图时，会把交叉轴压缩到 `1~3px` 细线，
//! 叠加 `Nearest` 采样导致侧边预览彻底丧失可辨识度。本模块采用两级自适应策略：
//! 1. **常规长宽比**：完整等比缩放整张画布，优先保证用户一眼看到全局拼接轮廓；
//! 2. **极端长宽比**：当整图缩至目标容器会使交叉轴低于 `min_readable_cross` 时，切换为
//!    “保持交叉轴清晰度的尾部最近拼接切片窗”，同时返回画布归一化偏移 `[slice_start_ratio, 1.0]`，
//!    既让用户在进行尾部裁剪时能看清最新内容与文档边界，又能绘制全局进度条反映总进度。
//! 3. **双阶段降采样**：当源图尺寸远超目标像素 `4x` 时，先执行步长中心采样预缩小，
//!    再做双线性 `Triangle` 重采样，在保证文字/边框可读性的同时把单次耗时控制在毫秒级。

use super::types::{StitchDirection, ThumbnailOverview};
use image::{imageops, GenericImageView, Rgba, RgbaImage};

/// 按方向、激活生长端与目标槽位尺寸生成带局部映射信息的实时预览缩略图。
pub(crate) fn render_thumbnail_overview(
    canvas: &RgbaImage,
    direction: StitchDirection,
    max_w: u32,
    max_h: u32,
    min_readable_cross: u32,
    anchor_at_head: bool,
) -> ThumbnailOverview {
    let (cw, ch) = canvas.dimensions();
    if cw == 0 || ch == 0 || max_w == 0 || max_h == 0 {
        return ThumbnailOverview {
            image: RgbaImage::new(1, 1),
            slice_start_ratio: 0.0,
            slice_end_ratio: 1.0,
            is_cropped_window: false,
        };
    }

    let (main_span, cross_span, max_main, max_cross) = match direction {
        StitchDirection::Vertical => (ch, cw, max_h, max_w),
        StitchDirection::Horizontal => (cw, ch, max_w, max_h),
    };

    let full_scale = ((max_w as f32) / (cw as f32))
        .min((max_h as f32) / (ch as f32))
        .min(1.0);
    let full_cross = ((cross_span as f32) * full_scale).round() as u32;
    let eff_min_cross = min_readable_cross.min(max_cross).min(cross_span);

    if eff_min_cross > 0 && full_cross < eff_min_cross {
        let window_scale = (eff_min_cross as f32 / cross_span as f32).min(1.0);
        let src_main_window = ((max_main as f32) / window_scale)
            .floor()
            .max(1.0)
            .min(main_span as f32) as u32;

        if src_main_window < main_span {
            let src_start = if anchor_at_head {
                0
            } else {
                main_span.saturating_sub(src_main_window)
            };
            let src_end = (src_start + src_main_window).min(main_span);
            let thumb_cross = eff_min_cross.max(1);
            let thumb_main = ((src_main_window as f32) * window_scale)
                .round()
                .clamp(1.0, max_main as f32) as u32;

            let (crop_x, crop_y, crop_w, crop_h, thumb_w, thumb_h) = match direction {
                StitchDirection::Vertical => {
                    (0, src_start, cw, src_main_window, thumb_cross, thumb_main)
                }
                StitchDirection::Horizontal => {
                    (src_start, 0, src_main_window, ch, thumb_main, thumb_cross)
                }
            };
            let image = downsample_region(
                canvas,
                SrcRegion::new(crop_x, crop_y, crop_w, crop_h),
                thumb_w,
                thumb_h,
            );
            return ThumbnailOverview {
                image,
                slice_start_ratio: (src_start as f32 / main_span as f32).clamp(0.0, 1.0),
                slice_end_ratio: (src_end as f32 / main_span as f32).clamp(0.0, 1.0),
                is_cropped_window: true,
            };
        }
    }

    let thumb_w = ((cw as f32) * full_scale).round().clamp(1.0, max_w as f32) as u32;
    let thumb_h = ((ch as f32) * full_scale).round().clamp(1.0, max_h as f32) as u32;
    ThumbnailOverview {
        image: downsample_region(canvas, SrcRegion::new(0, 0, cw, ch), thumb_w, thumb_h),
        slice_start_ratio: 0.0,
        slice_end_ratio: 1.0,
        is_cropped_window: false,
    }
}

#[derive(Clone, Copy)]
struct SrcRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl SrcRegion {
    const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// 高速双阶段降采样：超大图直接零拷贝读取源像素做 2x 预抽样，再经 `Triangle` 滤波平滑缩放。
fn downsample_region(
    canvas: &RgbaImage,
    src: SrcRegion,
    target_w: u32,
    target_h: u32,
) -> RgbaImage {
    let dst_w = target_w.max(1);
    let dst_h = target_h.max(1);

    if src.width == dst_w && src.height == dst_h {
        return canvas.view(src.x, src.y, dst_w, dst_h).to_image();
    }

    // 超出目标尺寸 4 倍以上时直接稀疏四点采样预缩至 2 倍目标尺寸，避免拷贝巨型视图到临时堆内存。
    if src.width > dst_w.saturating_mul(4) || src.height > dst_h.saturating_mul(4) {
        let mid_w = dst_w.saturating_mul(2).min(src.width).max(1);
        let mid_h = dst_h.saturating_mul(2).min(src.height).max(1);
        let mut mid = RgbaImage::new(mid_w, mid_h);
        for my in 0..mid_h {
            let sy0 = ((u64::from(my) * u64::from(src.height)) / u64::from(mid_h)) as u32;
            let sy1 = (((u64::from(my) + 1) * u64::from(src.height)) / u64::from(mid_h))
                .max(u64::from(sy0) + 1)
                .min(u64::from(src.height)) as u32;
            let sy_mid = (sy0 + (sy1 - sy0) / 2).min(src.height - 1);
            for mx in 0..mid_w {
                let sx0 = ((u64::from(mx) * u64::from(src.width)) / u64::from(mid_w)) as u32;
                let sx1 = (((u64::from(mx) + 1) * u64::from(src.width)) / u64::from(mid_w))
                    .max(u64::from(sx0) + 1)
                    .min(u64::from(src.width)) as u32;
                let sx_mid = (sx0 + (sx1 - sx0) / 2).min(src.width - 1);

                let p0 = canvas.get_pixel(src.x + sx0, src.y + sy0).0;
                let p1 = canvas.get_pixel(src.x + sx_mid, src.y + sy0).0;
                let p2 = canvas.get_pixel(src.x + sx0, src.y + sy_mid).0;
                let p3 = canvas.get_pixel(src.x + sx_mid, src.y + sy_mid).0;
                let avg = [
                    ((u16::from(p0[0]) + u16::from(p1[0]) + u16::from(p2[0]) + u16::from(p3[0]))
                        / 4) as u8,
                    ((u16::from(p0[1]) + u16::from(p1[1]) + u16::from(p2[1]) + u16::from(p3[1]))
                        / 4) as u8,
                    ((u16::from(p0[2]) + u16::from(p1[2]) + u16::from(p2[2]) + u16::from(p3[2]))
                        / 4) as u8,
                    ((u16::from(p0[3]) + u16::from(p1[3]) + u16::from(p2[3]) + u16::from(p3[3]))
                        / 4) as u8,
                ];
                mid.put_pixel(mx, my, Rgba(avg));
            }
        }
        return imageops::resize(&mid, dst_w, dst_h, imageops::FilterType::Triangle);
    }

    let view_owned = canvas.view(src.x, src.y, src.width, src.height).to_image();
    imageops::resize(&view_owned, dst_w, dst_h, imageops::FilterType::Triangle)
}
