//! 长截图侧边实时缩略图生成与极端长宽比自适应窗口裁剪算法。
//!
//! # 设计原理
//!
//! 传统整图等比缩放面对 `800×40000` 这类超长网页或日志长图时，会把交叉轴压缩到 `1~3px` 细线，
//! 叠加 `Nearest` 采样导致侧边预览彻底丧失可辨识度。本模块采用两级自适应策略：
//! 1. **常规长宽比**：完整等比缩放整张画布，优先保证用户一眼看到全局拼接轮廓；
//! 2. **极端长宽比**：当整图缩至目标容器会使交叉轴低于 `min_readable_cross` 时，切换为
//!    “保持交叉轴清晰度的激活端最近拼接切片窗”，同时返回画布归一化偏移 `[slice_start_ratio, slice_end_ratio]`，
//!    既让用户在进行头/尾裁剪时能看清最新内容与文档边界，又能绘制全局进度条反映总进度；
//! 3. **直读分块零物化降采样**：直接对 [`PagedCanvasSnapshot`] 执行跨块坐标抽样，无需整图拼装即可在后台线程毫秒级生成。

use super::paged_canvas::PagedCanvasSnapshot;
use super::stitcher::LongCaptureStitcher;
use super::types::{GrowthPolarity, StitchDirection, ThumbnailOverview};
use image::{imageops, Rgba, RgbaImage};

/// 侧边缩略图缓存数据结构，保存最近渲染的高清概览及对应规格键。
#[derive(Debug, Clone)]
pub(crate) struct ThumbnailCache {
    pub(crate) overview: ThumbnailOverview,
    pub(crate) canvas_dim: (u32, u32),
    pub(crate) spec: ThumbnailRenderSpec,
}

impl LongCaptureStitcher {
    /// 构建当前状态下的缩略图规格对象，供精确缓存命中校验或后台渲染派发。
    pub const fn build_thumbnail_spec(
        &self,
        max_w: u32,
        max_h: u32,
        min_readable_cross: u32,
    ) -> ThumbnailRenderSpec {
        ThumbnailRenderSpec {
            max_w,
            max_h,
            min_readable_cross,
            anchor_at_head: matches!(self.growth_polarity, Some(GrowthPolarity::Backward)),
        }
    }

    /// 若当前已有规格及画布尺寸完全匹配的缩略图缓存则直接返回（零 CPU 开销）。
    pub fn cached_thumbnail_overview(
        &self,
        max_w: u32,
        max_h: u32,
        min_readable_cross: u32,
    ) -> Option<&ThumbnailOverview> {
        let expected_dim = self.dimensions();
        let expected_spec = self.build_thumbnail_spec(max_w, max_h, min_readable_cross);
        self.thumbnail_cache
            .as_ref()
            .filter(|c| c.canvas_dim == expected_dim && c.spec == expected_spec)
            .map(|c| &c.overview)
    }

    /// 返回最近一次完成渲染的缩略图（即便画布正在快速增长中也能先保证零闪烁平滑过渡）。
    pub fn stale_thumbnail_overview(&self) -> Option<&ThumbnailOverview> {
        self.thumbnail_cache.as_ref().map(|c| &c.overview)
    }

    /// 写入后台线程完成渲染的缩略图结果；保持最新完成版本可见以消除快速滚动时面板空白。
    pub fn install_thumbnail_cache(
        &mut self,
        canvas_dim: (u32, u32),
        spec: ThumbnailRenderSpec,
        overview: ThumbnailOverview,
    ) {
        self.thumbnail_cache = Some(ThumbnailCache {
            overview,
            canvas_dim,
            spec,
        });
    }

    /// 获取或按需生成带局部窗口比例映射的高可读缩略图概览（同步阻塞路径与缓存兜底）。
    pub fn get_or_render_thumbnail_overview(
        &mut self,
        max_w: u32,
        max_h: u32,
        min_readable_cross: u32,
    ) -> &ThumbnailOverview {
        let canvas_dim = self.dimensions();
        let spec = self.build_thumbnail_spec(max_w, max_h, min_readable_cross);
        let need_rebuild = self
            .thumbnail_cache
            .as_ref()
            .is_none_or(|cached| cached.canvas_dim != canvas_dim || cached.spec != spec);
        if need_rebuild {
            let snapshot = self.canvas.snapshot();
            let overview = render_thumbnail_overview(&snapshot, spec);
            let entry = self.thumbnail_cache.insert(ThumbnailCache {
                overview,
                canvas_dim,
                spec,
            });
            return &entry.overview;
        }
        let snapshot = self.canvas.snapshot();
        &self
            .thumbnail_cache
            .get_or_insert_with(|| ThumbnailCache {
                overview: render_thumbnail_overview(&snapshot, spec),
                canvas_dim,
                spec,
            })
            .overview
    }

    /// 获取或按需生成整图等比缩放的缩略图位图（带尺寸缓存）。
    pub fn get_or_render_thumbnail(&mut self, max_w: u32, max_h: u32) -> &RgbaImage {
        &self.get_or_render_thumbnail_overview(max_w, max_h, 0).image
    }
}

/// 侧边缩略图生成规格参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbnailRenderSpec {
    /// 缩略图允许的最大宽度（像素）
    pub max_w: u32,
    /// 缩略图允许的最大高度（像素）
    pub max_h: u32,
    /// 交叉轴最小可读厚度（像素）；低于此值时切换为可读切片窗
    pub min_readable_cross: u32,
    /// 是否将可读切片窗锚定在画布首端（反向前插场景为 `true`，正向扩展场景为 `false`）
    pub anchor_at_head: bool,
}

/// 基于零拷贝分块画布快照按给定规格渲染侧边高可读缩略图概览。
pub fn render_thumbnail_overview(
    snapshot: &PagedCanvasSnapshot,
    spec: ThumbnailRenderSpec,
) -> ThumbnailOverview {
    let (cw, ch) = snapshot.dimensions();
    let direction = snapshot.direction();
    if cw == 0 || ch == 0 || spec.max_w == 0 || spec.max_h == 0 {
        return ThumbnailOverview {
            image: RgbaImage::new(1, 1),
            slice_start_ratio: 0.0,
            slice_end_ratio: 1.0,
            is_cropped_window: false,
        };
    }

    let (main_span, cross_span, max_main, max_cross) = match direction {
        StitchDirection::Vertical => (ch, cw, spec.max_h, spec.max_w),
        StitchDirection::Horizontal => (cw, ch, spec.max_w, spec.max_h),
    };

    let full_scale = ((spec.max_w as f32) / (cw as f32))
        .min((spec.max_h as f32) / (ch as f32))
        .min(1.0);
    let full_cross = ((cross_span as f32) * full_scale).round() as u32;
    let eff_min_cross = spec.min_readable_cross.min(max_cross).min(cross_span);

    if eff_min_cross > 0 && full_cross < eff_min_cross {
        let window_scale = (eff_min_cross as f32 / cross_span as f32).min(1.0);
        let src_main_window = ((max_main as f32) / window_scale)
            .floor()
            .max(1.0)
            .min(main_span as f32) as u32;

        if src_main_window < main_span {
            let src_start = if spec.anchor_at_head {
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
            let image = downsample_snapshot_region(
                snapshot,
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

    let thumb_w = ((cw as f32) * full_scale)
        .round()
        .clamp(1.0, spec.max_w as f32) as u32;
    let thumb_h = ((ch as f32) * full_scale)
        .round()
        .clamp(1.0, spec.max_h as f32) as u32;
    ThumbnailOverview {
        image: downsample_snapshot_region(snapshot, SrcRegion::new(0, 0, cw, ch), thumb_w, thumb_h),
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

/// 高速双阶段降采样：超大图直读分块坐标做 2x 四点预抽样，再经 `Triangle` 滤波平滑缩放。
fn downsample_snapshot_region(
    snapshot: &PagedCanvasSnapshot,
    src: SrcRegion,
    target_w: u32,
    target_h: u32,
) -> RgbaImage {
    let dst_w = target_w.max(1);
    let dst_h = target_h.max(1);

    if src.width == dst_w && src.height == dst_h {
        return extract_snapshot_rect(snapshot, src);
    }

    // 超出目标尺寸 4 倍以上时直接跨分块稀疏四点采样预缩至 2 倍目标尺寸，避免拷贝整张画布。
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

                let p0 = snapshot.get_pixel_xy(src.x + sx0, src.y + sy0);
                let p1 = snapshot.get_pixel_xy(src.x + sx_mid, src.y + sy0);
                let p2 = snapshot.get_pixel_xy(src.x + sx0, src.y + sy_mid);
                let p3 = snapshot.get_pixel_xy(src.x + sx_mid, src.y + sy_mid);
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

    let region = extract_snapshot_rect(snapshot, src);
    imageops::resize(&region, dst_w, dst_h, imageops::FilterType::Triangle)
}

fn extract_snapshot_rect(snapshot: &PagedCanvasSnapshot, src: SrcRegion) -> RgbaImage {
    match snapshot.direction() {
        StitchDirection::Vertical => snapshot.slice_axis(src.y, src.height),
        StitchDirection::Horizontal => snapshot.slice_axis(src.x, src.width),
    }
}
