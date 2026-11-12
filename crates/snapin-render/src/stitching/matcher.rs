//! 基于时域行切片、SAD（差分绝对值）与固定顶栏剔除的单轴位移估计器。

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

/// 探测前后两帧在画面顶部的固定静止行数（如微信窗口群名、网页 Sticky Header）。
///
/// 从第 0 行自上而下逐行对比。若两帧对应行的平均差分低于阈值，则判定为静止固定行。
pub(crate) fn detect_vertical_sticky_header(
    prev: &[u8],
    curr: &[u8],
    width: usize,
    height: usize,
    max_header_ratio: f32,
    x_start: usize,
    x_end: usize,
) -> usize {
    let max_rows = ((height as f32) * max_header_ratio).min(height as f32) as usize;
    let row_len = (x_end - x_start).max(1);
    let mut sticky_rows = 0;

    for y in 0..max_rows {
        let row_offset = y * width;
        let mut row_diff_sum: u64 = 0;
        for x in x_start..x_end {
            let p = prev[row_offset + x];
            let c = curr[row_offset + x];
            row_diff_sum += p.abs_diff(c) as u64;
        }
        let avg_diff = (row_diff_sum as f64) / (row_len as f64);
        // 单行平均像素差小于 3.5 视为固定不变
        if avg_diff <= 3.5 {
            sticky_rows += 1;
        } else {
            // 一旦出现明显变动的行，即判定为固定栏结束
            break;
        }
    }
    sticky_rows
}

/// 计算两行灰度切片在指定 X 范围内的平均差分绝对值
#[inline]
fn compute_row_sad(
    prev: &[u8],
    curr: &[u8],
    prev_y: usize,
    curr_y: usize,
    width: usize,
    x_start: usize,
    x_end: usize,
) -> u64 {
    let p_offset = prev_y * width;
    let c_offset = curr_y * width;
    let mut sum: u64 = 0;
    for x in x_start..x_end {
        let p = prev[p_offset + x];
        let c = curr[c_offset + x];
        sum += p.abs_diff(c) as u64;
    }
    sum
}

/// 垂直主方向单轴位移估计
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

    let sticky = detect_vertical_sticky_header(
        prev,
        curr,
        width,
        height,
        options.sticky_header_max_ratio,
        x_start,
        x_end,
    );

    let search_start_y = sticky;
    let active_height = height.saturating_sub(search_start_y);
    if active_height <= (options.min_step as usize).saturating_mul(2) {
        return MatchResult::idle();
    }

    let max_shift = (options.max_step as usize).min(active_height / 2);
    let min_shift = options.min_step as usize;

    let mut best_shift: i32 = 0;
    let mut min_avg_error = f64::MAX;

    // 1. 优先搜索正向位移（向下滚动）：内容向上移动，即 curr(y) 匹配 prev(y + shift)
    for shift in min_shift..=max_shift {
        let overlap_rows = active_height.saturating_sub(shift);
        if overlap_rows < 15 {
            continue;
        }
        let mut total_sad: u64 = 0;
        // 采样对比行数（隔行抽样以进一步提升运算性能）
        let step_y = if overlap_rows > 120 { 2 } else { 1 };
        let mut sampled_rows = 0;

        for r in (0..overlap_rows).step_by(step_y) {
            let curr_y = search_start_y + r;
            let prev_y = search_start_y + r + shift;
            total_sad += compute_row_sad(prev, curr, prev_y, curr_y, width, x_start, x_end);
            sampled_rows += 1;
        }

        let total_pixels = (sampled_rows * (x_end - x_start)).max(1) as f64;
        let avg_error = (total_sad as f64) / total_pixels;

        if avg_error < min_avg_error {
            min_avg_error = avg_error;
            best_shift = shift as i32;
        }
    }

    // 2. 当正向未找到近似零误差对齐时检测反向滚动：即使调用方关闭了自动反向裁剪，
    // 仍须由匹配器如实返回负位移，供上层区分“按策略忽略的回滚”与“真失配或正向强凑假阳性”。
    if min_avg_error > 8.0 || best_shift == 0 {
        for shift in min_shift..=(max_shift.min(80)) {
            let overlap_rows = active_height.saturating_sub(shift);
            if overlap_rows < 15 {
                continue;
            }
            let mut total_sad: u64 = 0;
            let step_y = if overlap_rows > 120 { 2 } else { 1 };
            let mut sampled_rows = 0;

            for r in (0..overlap_rows).step_by(step_y) {
                let curr_y = search_start_y + r + shift;
                let prev_y = search_start_y + r;
                total_sad += compute_row_sad(prev, curr, prev_y, curr_y, width, x_start, x_end);
                sampled_rows += 1;
            }

            let total_pixels = (sampled_rows * (x_end - x_start)).max(1) as f64;
            let avg_error = (total_sad as f64) / total_pixels;

            if avg_error < min_avg_error {
                min_avg_error = avg_error;
                best_shift = -(shift as i32);
            }
        }
    }

    // 转换为 0.0 ~ 1.0 的置信度
    let confidence = if min_avg_error < f64::MAX {
        ((255.0 - min_avg_error) / 255.0).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };

    MatchResult {
        offset: best_shift,
        confidence,
        sticky_header_size: sticky as u32,
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
