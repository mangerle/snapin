//! 基于时域二维空间分块裁剪 SAD 与固定双栏剔除的单轴位移估计器。

use super::overlay::detect_sticky_bands;
use super::types::{MatchResult, StitchDirection, StitchOptions};
use image::RgbaImage;

/// 快速单通道灰度亮度提取（基于标准 Rec.601 亮度加权系数）
#[inline]
pub(crate) fn rgb_to_luma(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) * 77 + u32::from(g) * 150 + u32::from(b) * 29) >> 8) as u8
}

/// 将 RGBA 图像转换为单通道灰度二维向量
pub(crate) fn to_luma_plane(image: &RgbaImage) -> (Vec<u8>, usize, usize) {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let raw = image.as_raw();
    let mut luma = Vec::with_capacity(width * height);
    for chunk in raw.as_chunks::<4>().0 {
        luma.push(rgb_to_luma(chunk[0], chunk[1], chunk[2]));
    }
    (luma, width, height)
}

/// 将图像转置为灰度平面，使横向滚动可以复用同一套单轴位移估计逻辑。
///
/// 横向页面通常具有固定左侧导航栏，转置后该导航栏会自然映射为纵向固定顶栏，
/// 从而保持粘性区域识别、抽样与置信度算法完全一致。代价是每次横向匹配需要额外
/// 分配一份转置灰度缓冲，但灰度数据远小于原始 RGBA 帧，且仅在横向长截图时启用。
fn to_luma_plane_transposed(image: &RgbaImage) -> (Vec<u8>, usize, usize) {
    let source_width = image.width() as usize;
    let source_height = image.height() as usize;
    let transposed_width = source_height;
    let transposed_height = source_width;
    let raw = image.as_raw();
    let mut luma = vec![0_u8; transposed_width.saturating_mul(transposed_height)];

    for y in 0..source_height {
        for x in 0..source_width {
            let source = (y.saturating_mul(source_width).saturating_add(x)).saturating_mul(4);
            let target = x.saturating_mul(transposed_width).saturating_add(y);
            if let Some(pixel) = raw.get(source..source.saturating_add(4)) {
                if let Some(slot) = luma.get_mut(target) {
                    *slot = rgb_to_luma(pixel[0], pixel[1], pixel[2]);
                }
            }
        }
    }

    (luma, transposed_width, transposed_height)
}

struct GridEvalSpec {
    width: usize,
    x_start: usize,
    x_end: usize,
    y_start: usize,
    active_height: usize,
    shift: i32,
}

/// 以 4×4 二维空间栅格计算对齐误差并只保留误差最小的前 75% 分块，
/// 彻底免疫局部悬浮按钮、Hover 卡片或动态角标对全局置信度的毒化，同时防止纯全局失配混入。
fn evaluate_trimmed_grid_error(prev: &[u8], curr: &[u8], spec: GridEvalSpec) -> f64 {
    let abs_shift = spec.shift.unsigned_abs() as usize;
    let overlap_rows = spec.active_height.saturating_sub(abs_shift);
    if overlap_rows < 15 || spec.x_end <= spec.x_start {
        return f64::MAX;
    }

    let roi_w = spec.x_end - spec.x_start;
    let cols = roi_w.min(4);
    let step_y = if overlap_rows > 120 { 2 } else { 1 };
    let mut grid_sad = [0u64; 16];
    let mut grid_px = [0usize; 16];

    for r in (0..overlap_rows).step_by(step_y) {
        let (prev_y, curr_y) = if spec.shift >= 0 {
            (spec.y_start + r + abs_shift, spec.y_start + r)
        } else {
            (spec.y_start + r, spec.y_start + r + abs_shift)
        };
        let row_band = ((r * 4) / overlap_rows).min(3);
        let p_base = prev_y * spec.width;
        let c_base = curr_y * spec.width;

        for c_idx in 0..cols {
            let x0 = spec.x_start + (roi_w * c_idx) / cols;
            let x1 = spec.x_start + (roi_w * (c_idx + 1)) / cols;
            let mut seg_sad = 0u64;
            for x in x0..x1 {
                seg_sad += u64::from(prev[p_base + x].abs_diff(curr[c_base + x]));
            }
            let cell = row_band * 4 + c_idx;
            grid_sad[cell] += seg_sad;
            grid_px[cell] += x1 - x0;
        }
    }

    let mut cell_errs = [0.0f64; 16];
    let mut count = 0usize;
    for idx in 0..16 {
        if grid_px[idx] > 0 {
            cell_errs[count] = (grid_sad[idx] as f64) / (grid_px[idx] as f64);
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

struct ShiftSearchSpec {
    width: usize,
    x_start: usize,
    x_end: usize,
    y_start: usize,
    active_height: usize,
    min_shift: usize,
    max_shift: usize,
    sign: i32,
}

fn search_best_shift(prev: &[u8], curr: &[u8], spec: ShiftSearchSpec) -> Option<(i32, f64)> {
    let mut best: Option<(i32, f64)> = None;
    for shift in spec.min_shift..=spec.max_shift {
        let signed = (shift as i32) * spec.sign;
        let err = evaluate_trimmed_grid_error(
            prev,
            curr,
            GridEvalSpec {
                width: spec.width,
                x_start: spec.x_start,
                x_end: spec.x_end,
                y_start: spec.y_start,
                active_height: spec.active_height,
                shift: signed,
            },
        );
        if best.is_none_or(|(_, best_err)| err < best_err) {
            best = Some((signed, err));
        }
    }
    best
}

/// 垂直主方向单轴位移估计（剔除固定头尾栏并对局部浮层遮挡免疫）
pub(crate) fn estimate_vertical_shift(
    prev: &[u8],
    curr: &[u8],
    width: usize,
    height: usize,
    options: &StitchOptions,
) -> MatchResult {
    if height < 10 || width < 10 {
        return MatchResult::idle();
    }
    let x_margin = ((width as f32) * options.margin_ratio).round() as usize;
    let x_start = x_margin.min(width / 4);
    let x_end = width
        .saturating_sub(x_margin)
        .max(x_start.saturating_add(1))
        .min(width);

    let bands = detect_sticky_bands(
        prev,
        curr,
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

    let max_shift = (options.max_step as usize).min(active_height / 2);
    let min_shift = options.min_step as usize;
    let (mut best_shift, mut min_avg_error) = search_best_shift(
        prev,
        curr,
        ShiftSearchSpec {
            width,
            x_start,
            x_end,
            y_start: bands.header,
            active_height,
            min_shift,
            max_shift,
            sign: 1,
        },
    )
    .unwrap_or((0, f64::MAX));

    if min_avg_error > 8.0 || best_shift == 0 {
        if let Some((rev_shift, rev_err)) = search_best_shift(
            prev,
            curr,
            ShiftSearchSpec {
                width,
                x_start,
                x_end,
                y_start: bands.header,
                active_height,
                min_shift,
                max_shift: max_shift.min(80),
                sign: -1,
            },
        ) {
            if rev_err < min_avg_error {
                min_avg_error = rev_err;
                best_shift = rev_shift;
            }
        }
    }

    let confidence = if min_avg_error < f64::MAX {
        ((255.0 - min_avg_error) / 255.0).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };

    MatchResult {
        offset: best_shift,
        confidence,
        sticky_header_size: bands.header as u32,
        sticky_footer_size: bands.footer as u32,
    }
}

/// 判定两帧在排除边缘滚动条干扰后的有效内容区域内是否视觉静止无滚动。
///
/// 实现初衷：若画面未滚动（完全相同或仅存在局部文本光标闪烁），直接跳过全量 SAD 搜索，
/// 避免纯色/低纹理静态画面被误判为 `min_step` 伪位移或触发无意义的匹配失败告警；
/// 同时按“单行变点数 `<= 4` 且总变点极少”早停退出，确保仅一两行微弱文本滚动也不会被吞掉。
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
    let roi_pixels = (x_end.saturating_sub(x_start)).saturating_mul(y_end.saturating_sub(y_start));
    let max_total_changed = (roi_pixels / 500).clamp(16, 64);

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
                if row_changed > 4 || total_changed > max_total_changed {
                    return false;
                }
            }
        }
    }
    true
}

/// 计算静止检测排除横向滚动条（垂直拼图）或底部进度条（横向拼图）后的 ROI 边界。
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
    match direction {
        StitchDirection::Vertical => {
            let (prev_luma, width, height) = to_luma_plane(prev_frame);
            let (curr_luma, _, _) = to_luma_plane(curr_frame);
            estimate_vertical_shift(&prev_luma, &curr_luma, width, height, options)
        }
        StitchDirection::Horizontal => {
            let (prev_luma, width, height) = to_luma_plane_transposed(prev_frame);
            let (curr_luma, _, _) = to_luma_plane_transposed(curr_frame);
            estimate_vertical_shift(&prev_luma, &curr_luma, width, height, options)
        }
    }
}
