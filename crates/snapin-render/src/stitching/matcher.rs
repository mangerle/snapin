//! 基于三通道（亮度+对立色差）2 级由粗到细金字塔、主轴梯度加权裁剪网格与 ZNCC 共识的单轴位移估计器。

use super::overlay::detect_sticky_bands;
use super::pyramid::{
    downsample_roi_4x2, feature_max_diff, pick_coarse_candidate_centers, verify_multi_band_zncc,
    ConsensusRoi, FeaturePlane,
};
use super::types::{MatchResult, StitchDirection, StitchOptions};
use image::RgbaImage;

/// 同一主峰局部极值盆地的半径（像素），用于剥离主峰邻域后再搜索次优竞争峰。
const PEAK_BASIN_RADIUS: i32 = 6;
/// 参与沿主轴梯度加权的最小纵向局部梯度，过滤传感器/渲染 1~2 LSB 抖动。
const MIN_EFFECTIVE_GRAD: u8 = 3;
/// 启用 `4×` 粗尺度金字塔预筛选的最小活跃区主轴跨度与搜索步长阈值。
const PYRAMID_MIN_ACTIVE_HEIGHT: usize = 160;
const PYRAMID_MIN_MAX_SHIFT: usize = 32;

/// 快速单通道灰度亮度提取（基于标准 Rec.601 亮度加权系数）
#[inline]
pub(crate) fn rgb_to_luma(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) * 77 + u32::from(g) * 150 + u32::from(b) * 29) >> 8) as u8
}

struct GridEvalContext<'a> {
    prev: &'a FeaturePlane,
    curr: &'a FeaturePlane,
    x_start: usize,
    x_end: usize,
    y_start: usize,
    active_height: usize,
}

#[derive(Default)]
struct GridAccumulators {
    weighted_sad: [u64; 16],
    weight_sum: [u64; 16],
    textured_px: [usize; 16],
    roi_samples: usize,
    textured_samples: usize,
    total_weight: u64,
}

impl GridAccumulators {
    fn has_sufficient_texture(&self) -> bool {
        let min_textured = (self.roi_samples / 150).max(16);
        let min_weight = (self.roi_samples as u64 / 12).max(128);
        self.textured_samples >= min_textured && self.total_weight >= min_weight
    }

    fn trimmed_error(&self) -> f64 {
        if !self.has_sufficient_texture() {
            return f64::MAX;
        }
        let mut cell_errs = [0.0f64; 16];
        let mut count = 0usize;
        for idx in 0..16 {
            if self.textured_px[idx] >= 2 && self.weight_sum[idx] >= 8 {
                cell_errs[count] = (self.weighted_sad[idx] as f64) / (self.weight_sum[idx] as f64);
                count += 1;
            }
        }
        if count == 0 {
            return f64::MAX;
        }
        cell_errs[..count].sort_by(|a, b| a.total_cmp(b));
        let keep = ((count * 3) / 4).max(1);
        let sum_keep: f64 = cell_errs[..keep].iter().sum();
        sum_keep / (keep as f64)
    }
}

/// 基于三通道正交基与沿主轴局部梯度加权，计算 4×4 空间栅格的 75% 裁剪平均误差。
fn evaluate_weighted_grid_error(ctx: &GridEvalContext<'_>, shift: i32) -> f64 {
    let abs_shift = shift.unsigned_abs() as usize;
    let overlap_rows = ctx.active_height.saturating_sub(abs_shift);
    if overlap_rows < 15 || ctx.x_end <= ctx.x_start {
        return f64::MAX;
    }

    let roi_w = ctx.x_end - ctx.x_start;
    let cols = roi_w.min(4);
    let step_y = if overlap_rows > 320 {
        3
    } else if overlap_rows > 120 {
        2
    } else {
        1
    };
    let mut acc = GridAccumulators::default();

    for r in (0..overlap_rows).step_by(step_y) {
        let (prev_y, curr_y) = if shift >= 0 {
            (ctx.y_start + r + abs_shift, ctx.y_start + r)
        } else {
            (ctx.y_start + r, ctx.y_start + r + abs_shift)
        };
        let row_band = ((r * 4) / overlap_rows).min(3);
        let p_base = prev_y * ctx.prev.width;
        let c_base = curr_y * ctx.curr.width;

        for c_idx in 0..cols {
            let x0 = ctx.x_start + (roi_w * c_idx) / cols;
            let x1 = ctx.x_start + (roi_w * (c_idx + 1)) / cols;
            accumulate_cell_row(
                ctx,
                &mut acc,
                (p_base, c_base),
                (x0, x1),
                row_band * 4 + c_idx,
            );
        }
    }

    acc.trimmed_error()
}

#[inline]
fn accumulate_cell_row(
    ctx: &GridEvalContext<'_>,
    acc: &mut GridAccumulators,
    (p_base, c_base): (usize, usize),
    (x0, x1): (usize, usize),
    cell: usize,
) {
    for x in x0..x1 {
        let p_idx = p_base + x;
        let c_idx = c_base + x;
        let w = ctx.prev.grad[p_idx].max(ctx.curr.grad[c_idx]);
        acc.roi_samples += 1;
        if w >= MIN_EFFECTIVE_GRAD {
            let weight = u64::from(w);
            let diff = u64::from(feature_max_diff(
                ctx.prev.pixels[p_idx],
                ctx.curr.pixels[c_idx],
            ));
            acc.weighted_sad[cell] += weight * diff;
            acc.weight_sum[cell] += weight;
            acc.textured_px[cell] += 1;
            acc.textured_samples += 1;
            acc.total_weight += weight;
        }
    }
}

fn eval_and_record(ctx: &GridEvalContext<'_>, shift: i32, scores: &mut Vec<(i32, f64)>) {
    if scores.iter().any(|(seen, _)| *seen == shift) {
        return;
    }
    let err = evaluate_weighted_grid_error(ctx, shift);
    scores.push((shift, err));
}

fn search_shifts_pyramid_or_dense(
    ctx: &GridEvalContext<'_>,
    min_shift: usize,
    max_shift: usize,
) -> (Vec<(i32, f64)>, (i32, f64)) {
    let mut scores = Vec::with_capacity(96);
    let can_pyramid =
        ctx.active_height >= PYRAMID_MIN_ACTIVE_HEIGHT && max_shift > PYRAMID_MIN_MAX_SHIFT;

    if can_pyramid {
        if let (Some(prev_c), Some(curr_c)) = (
            downsample_roi_4x2(
                ctx.prev,
                ctx.x_start,
                ctx.x_end,
                ctx.y_start,
                ctx.active_height,
            ),
            downsample_roi_4x2(
                ctx.curr,
                ctx.x_start,
                ctx.x_end,
                ctx.y_start,
                ctx.active_height,
            ),
        ) {
            let micro_hi = max_shift.min(10);
            for s in min_shift..=micro_hi {
                eval_and_record(ctx, s as i32, &mut scores);
                eval_and_record(ctx, -(s as i32), &mut scores);
            }
            for center in pick_coarse_candidate_centers(&prev_c, &curr_c, max_shift) {
                let sign = center.signum();
                let lo = (center.abs() - 3).max(min_shift as i32);
                let hi = (center.abs() + 3).min(max_shift as i32);
                for mag in lo..=hi {
                    eval_and_record(ctx, mag * sign, &mut scores);
                }
            }
            if let Some(best) = pick_best(&scores) {
                eval_and_record(ctx, -best.0, &mut scores);
                return (scores, best);
            }
        }
    }

    // 正反向全范围对称搜索：防止正向碰巧出现低误差伪峰时提前跳过真正的反向位移。
    for s in min_shift..=max_shift {
        eval_and_record(ctx, s as i32, &mut scores);
        eval_and_record(ctx, -(s as i32), &mut scores);
    }
    let best = pick_best(&scores).unwrap_or((0, f64::MAX));
    (scores, best)
}

fn pick_best(scores: &[(i32, f64)]) -> Option<(i32, f64)> {
    scores.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1))
}

fn find_second_best_outside_basin(scores: &[(i32, f64)], best_shift: i32) -> f64 {
    scores
        .iter()
        .filter(|(shift, _)| (shift - best_shift).abs() > PEAK_BASIN_RADIUS)
        .map(|(_, err)| *err)
        .fold(f64::MAX, f64::min)
}

/// 验证获胜位移是否真正优于静止帧、且未落入周期性重复纹理的多义陷阱。
fn passes_ambiguity_guards(best_err: f64, err_zero: f64, second_err: f64) -> bool {
    if !best_err.is_finite() || best_err == f64::MAX || !err_zero.is_finite() {
        return false;
    }
    if err_zero < best_err * 1.4 + 2.5 {
        return false;
    }
    if second_err < 25.0 && second_err < best_err * 1.8 + 6.0 {
        return false;
    }
    true
}

/// 垂直主方向单轴位移估计（融合 2 级金字塔、多通道色差边缘、重复拦截与 ZNCC 结构共识）
pub(crate) fn estimate_plane_shift(
    prev: &FeaturePlane,
    curr: &FeaturePlane,
    options: &StitchOptions,
) -> MatchResult {
    let (width, height) = (curr.width, curr.height);
    if height < 10 || width < 10 || prev.width != width || prev.height != height {
        return MatchResult::idle();
    }
    let x_margin = ((width as f32) * options.margin_ratio).round() as usize;
    let x_start = x_margin.min(width / 4);
    let x_end = width
        .saturating_sub(x_margin)
        .max(x_start.saturating_add(1))
        .min(width);

    let prev_luma = prev.luma_plane();
    let curr_luma = curr.luma_plane();
    let bands = detect_sticky_bands(
        &prev_luma,
        &curr_luma,
        width,
        height,
        options.sticky_header_max_ratio,
        x_start,
        x_end,
    );
    let active_height = height
        .saturating_sub(bands.header)
        .saturating_sub(bands.footer);
    if active_height <= (options.min_step as usize).saturating_mul(2) {
        return MatchResult::idle();
    }

    let ctx = GridEvalContext {
        prev,
        curr,
        x_start,
        x_end,
        y_start: bands.header,
        active_height,
    };

    let err_zero = evaluate_weighted_grid_error(&ctx, 0);
    if err_zero == f64::MAX {
        return MatchResult::idle();
    }

    let min_overlap = (active_height / 4).max(24).min(active_height);
    let adaptive_max = active_height.saturating_sub(min_overlap);
    let max_shift = if options.max_step == 0 {
        adaptive_max
    } else {
        (options.max_step as usize).min(adaptive_max)
    };
    let min_shift = options.min_step as usize;
    if max_shift < min_shift {
        return MatchResult::idle();
    }

    let (scores, (best_shift, min_avg_error)) =
        search_shifts_pyramid_or_dense(&ctx, min_shift, max_shift);
    let second_err = find_second_best_outside_basin(&scores, best_shift);
    if !passes_ambiguity_guards(min_avg_error, err_zero, second_err) {
        return MatchResult::idle();
    }

    let consensus_ok = verify_multi_band_zncc(
        prev,
        curr,
        &ConsensusRoi {
            x_start,
            x_end,
            y_start: bands.header,
            active_height,
            shift: best_shift,
        },
    );
    if !consensus_ok {
        return MatchResult::idle();
    }

    let confidence = (1.0 - min_avg_error / 100.0).clamp(0.0, 1.0) as f32;
    MatchResult {
        offset: best_shift,
        confidence,
        sticky_header_size: bands.header as u32,
        sticky_footer_size: bands.footer as u32,
    }
}

/// 判定两帧在排除边缘滚动条干扰后的有效内容区域内是否视觉静止无滚动。
pub(crate) fn frames_visually_unchanged(
    prev: &RgbaImage,
    curr: &RgbaImage,
    direction: StitchDirection,
    options: &StitchOptions,
) -> bool {
    if prev.dimensions() != curr.dimensions() {
        return false;
    }
    let (w, h) = (prev.width() as usize, prev.height() as usize);
    if w == 0 || h == 0 {
        return true;
    }
    let (x_start, x_end, y_start, y_end) = active_static_roi(w, h, direction, options.margin_ratio);
    let roi_w = x_end.saturating_sub(x_start);
    let roi_pixels = roi_w.saturating_mul(y_end.saturating_sub(y_start));
    // 按有效选区面积与行宽等比放大容差上限，避免大尺寸选区下微小光标/角标抖动误触发全帧金字塔搜索。
    let max_total_changed = (roi_pixels / 500).clamp(16, 1024);
    let max_row_changed = (roi_w / 48).clamp(4, 32);

    let prev_raw = prev.as_raw();
    let curr_raw = curr.as_raw();
    let stride = w * 4;
    let mut total_changed = 0usize;

    for y in y_start..y_end {
        let row_base = y * stride;
        let mut row_changed = 0usize;
        for x in x_start..x_end {
            let idx = row_base + x * 4;
            let dr = prev_raw[idx].abs_diff(curr_raw[idx]);
            let dg = prev_raw[idx + 1].abs_diff(curr_raw[idx + 1]);
            let db = prev_raw[idx + 2].abs_diff(curr_raw[idx + 2]);
            if dr > 2 || dg > 2 || db > 2 {
                row_changed += 1;
                total_changed += 1;
                if row_changed > max_row_changed || total_changed > max_total_changed {
                    return false;
                }
            }
        }
    }
    true
}

fn active_static_roi(
    width: usize,
    height: usize,
    direction: StitchDirection,
    margin_ratio: f32,
) -> (usize, usize, usize, usize) {
    match direction {
        StitchDirection::Vertical => {
            let margin = ((width as f32) * margin_ratio).round() as usize;
            let x_start = margin.min(width / 4);
            let x_end = width
                .saturating_sub(x_start)
                .max(x_start.saturating_add(1))
                .min(width);
            (x_start, x_end, 0, height)
        }
        StitchDirection::Horizontal => {
            let margin = ((height as f32) * margin_ratio).round() as usize;
            let y_start = margin.min(height / 4);
            let y_end = height
                .saturating_sub(y_start)
                .max(y_start.saturating_add(1))
                .min(height);
            (0, width, y_start, y_end)
        }
    }
}

/// 估算两帧之间的单轴平移位移
pub fn estimate_shift(
    prev_frame: &RgbaImage,
    curr_frame: &RgbaImage,
    direction: StitchDirection,
    options: &StitchOptions,
) -> MatchResult {
    if prev_frame.dimensions() != curr_frame.dimensions() {
        return MatchResult::idle();
    }
    let (w, h) = (prev_frame.width() as usize, prev_frame.height() as usize);
    let (prev_plane, curr_plane) = match direction {
        StitchDirection::Vertical => (
            FeaturePlane::from_rgba(prev_frame.as_raw(), w, h),
            FeaturePlane::from_rgba(curr_frame.as_raw(), w, h),
        ),
        StitchDirection::Horizontal => (
            FeaturePlane::from_rgba_transposed(prev_frame.as_raw(), w, h),
            FeaturePlane::from_rgba_transposed(curr_frame.as_raw(), w, h),
        ),
    };
    estimate_plane_shift(&prev_plane, &curr_plane, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_dense_search_evaluates_negative_shifts_symmetrically() {
        // 构造 active_height < 160（走全分辨率稠密搜索分支）的场景：
        // 仅因局部微小扰动令正向 +24 呈现近零误差，而真实位移为反向 -16（err = 0）。
        let width = 120u32;
        let height = 140u32;
        let prev = RgbaImage::from_fn(width, height, |x, y| {
            let band = ((y / 8) * 37 + (x / 10) * 19) % 200 + 25;
            let fine = ((y * 17 + x * 11) % 60) as u8;
            Rgba([
                band as u8,
                band.saturating_add(fine as u32) as u8,
                (255 - band) as u8,
                255,
            ])
        });
        // curr 由 prev 向上滚动 16px（真实 offset = -16），且在部分区域外不存在正向误匹配。
        let curr = RgbaImage::from_fn(width, height, |x, y| {
            if y >= 16 {
                *prev.get_pixel(x, y - 16)
            } else {
                let v = ((x * 29 + y * 13) % 180 + 30) as u8;
                Rgba([v, 220 - v / 2, v / 2 + 40, 255])
            }
        });

        let options = StitchOptions::default();
        let res = estimate_shift(&prev, &curr, StitchDirection::Vertical, &options);
        assert!(res.is_matched(options.confidence_threshold));
        assert_eq!(res.offset, -16);
    }

    #[test]
    fn test_large_viewport_static_detection_tolerates_small_indicator_flicker() {
        let width = 1280u32;
        let height = 720u32;
        let prev = RgbaImage::from_pixel(width, height, Rgba([245, 246, 248, 255]));
        let mut curr = prev.clone();
        // 16x16 动态加载微标 (256px) 位于大选区内：应视为画面静止而免除高耗时匹配
        for y in 120..136 {
            for x in 400..416 {
                curr.put_pixel(x, y, Rgba([60, 130, 240, 255]));
            }
        }
        let options = StitchOptions::default();
        assert!(frames_visually_unchanged(
            &prev,
            &curr,
            StitchDirection::Vertical,
            &options
        ));

        // 超过单行与总面积容差的真实滚动文本行仍应立即检出变化
        for y in 300..308 {
            for x in 200..360 {
                curr.put_pixel(x, y, Rgba([30, 30, 35, 255]));
            }
        }
        assert!(!frames_visually_unchanged(
            &prev,
            &curr,
            StitchDirection::Vertical,
            &options
        ));
    }
}
