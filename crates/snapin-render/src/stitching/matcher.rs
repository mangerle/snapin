//! 基于沿主轴梯度加权、空间分块裁剪与峰值独特性验证的单轴位移估计器。

use super::overlay::detect_sticky_bands;
use super::types::{MatchResult, StitchDirection, StitchOptions};
use image::RgbaImage;

/// 同一主峰局部极值盆地的半径（像素），用于剥离主峰邻域后再搜索次优竞争峰。
const PEAK_BASIN_RADIUS: i32 = 6;
/// 参与沿主轴梯度加权的最小纵向局部梯度，过滤传感器/渲染 1~2 LSB 抖动。
const MIN_EFFECTIVE_GRAD: u8 = 3;

/// 快速单通道灰度亮度提取（基于标准 Rec.601 亮度加权系数）
#[inline]
pub(crate) fn rgb_to_luma(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) * 77 + u32::from(g) * 150 + u32::from(b) * 29) >> 8) as u8
}

/// 预计算沿滚动主轴方向的一阶绝对梯度图（`|I(y, x) - I(y-1, x)|`）。
///
/// 实现初衷：平行于滚动方向的纯竖线（对垂直滚动而言）在位移前后完全不变，无法锁定位移；
/// 只有沿主滚动方向存在灰度变化的结构边缘才具备空间定位信息。预计算一次梯度图并在后续
/// 全部候选位移评分中复用，避免外层循环重复邻域寻址。
fn compute_axis_gradients(luma: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut grad = vec![0u8; width.saturating_mul(height)];
    for y in 1..height {
        let row = y * width;
        let prev_row = (y - 1) * width;
        for x in 0..width {
            grad[row + x] = luma[row + x].abs_diff(luma[prev_row + x]);
        }
    }
    grad
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

struct GridEvalContext<'a> {
    prev: &'a [u8],
    curr: &'a [u8],
    grad_prev: &'a [u8],
    grad_curr: &'a [u8],
    width: usize,
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
            // 跳过无特征空白格，避免大面积留白稀释失配误差或吞掉 75% 裁剪名额。
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

/// 基于沿主轴局部梯度加权计算 4×4 空间栅格的 75% 裁剪平均误差。
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
        let p_base = prev_y * ctx.width;
        let c_base = curr_y * ctx.width;

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
        let w = ctx.grad_prev[p_idx].max(ctx.grad_curr[c_idx]);
        acc.roi_samples += 1;
        if w >= MIN_EFFECTIVE_GRAD {
            let weight = u64::from(w);
            let diff = u64::from(ctx.prev[p_idx].abs_diff(ctx.curr[c_idx]));
            acc.weighted_sad[cell] += weight * diff;
            acc.weight_sum[cell] += weight;
            acc.textured_px[cell] += 1;
            acc.textured_samples += 1;
            acc.total_weight += weight;
        }
    }
}

fn collect_shift_scores(
    ctx: &GridEvalContext<'_>,
    min_shift: usize,
    max_shift: usize,
    sign: i32,
    out: &mut Vec<(i32, f64)>,
) -> Option<(i32, f64)> {
    let mut best: Option<(i32, f64)> = None;
    for shift in min_shift..=max_shift {
        let signed = (shift as i32) * sign;
        let err = evaluate_weighted_grid_error(ctx, signed);
        out.push((signed, err));
        if best.is_none_or(|(_, best_err)| err < best_err) {
            best = Some((signed, err));
        }
    }
    best
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
    // 静止基准防漂移：有效正文未发生明显位移时，禁止强行挑选最小步长作为伪滚动。
    if err_zero < best_err * 1.4 + 2.5 {
        return false;
    }
    // 周期重复纹理峰独特性比值检查（Lowe's Ratio 思想）：
    // 若在主峰邻域外仍存在误差极接近的竞争位移，说明画面为等周期重复条纹或相同骨架，放弃猜测。
    if second_err < 25.0 && second_err < best_err * 1.8 + 6.0 {
        return false;
    }
    true
}

/// 垂直主方向单轴位移估计（剔除固定头尾栏、抵抗局部浮层与重复纯色误匹配）
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

    let grad_prev = compute_axis_gradients(prev, width, height);
    let grad_curr = compute_axis_gradients(curr, width, height);
    let ctx = GridEvalContext {
        prev,
        curr,
        grad_prev: &grad_prev,
        grad_curr: &grad_curr,
        width,
        x_start,
        x_end,
        y_start: bands.header,
        active_height,
    };

    let err_zero = evaluate_weighted_grid_error(&ctx, 0);
    if err_zero == f64::MAX {
        return MatchResult::idle();
    }

    // 依据有效活跃区跨度自适应决定最大允许位移（默认保留至少 25% 且不少于 24px 的重叠区），
    // 并对正反向搜索统一对称使用同一最大位移上界，不再硬锁 300px 与 80px。
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
    let mut scores = Vec::with_capacity(max_shift.saturating_mul(2));

    let (mut best_shift, mut min_avg_error) =
        collect_shift_scores(&ctx, min_shift, max_shift, 1, &mut scores).unwrap_or((0, f64::MAX));

    if min_avg_error > 2.0 || best_shift == 0 {
        if let Some((rev_shift, rev_err)) =
            collect_shift_scores(&ctx, min_shift, max_shift, -1, &mut scores)
        {
            if rev_err < min_avg_error {
                min_avg_error = rev_err;
                best_shift = rev_shift;
            }
        }
    } else if (best_shift.unsigned_abs() as usize) >= min_shift
        && (best_shift.unsigned_abs() as usize) <= max_shift
    {
        // 追加镜像反向位移点作为半周期周期纹理反对称歧义校验，成本仅 O(1)。
        let mirror = -best_shift;
        scores.push((mirror, evaluate_weighted_grid_error(&ctx, mirror)));
    }

    let second_err = find_second_best_outside_basin(&scores, best_shift);
    if !passes_ambiguity_guards(min_avg_error, err_zero, second_err) {
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
