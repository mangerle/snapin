//! 固定页眉/页脚检测与局部静态浮动浮层（悬浮球、角标、Hover 卡片）遮挡自愈。
//!
//! # 设计原理
//!
//! 现代桌面窗口与网页普遍存在两类静态干扰物：
//! 1. **通栏固定页眉与页脚**：位于视窗首尾两端，滚动期间像素恒定且存在 UI 结构纹理；
//!    `detect_sticky_bands` 将匹配活跃区收窄至两栏之间的真实正文窗口，同时暴露厚度给
//!    `stitcher` 以在末端动态替换旧页脚、只保留终图单一页脚且零吞行正文。
//! 2. **局部浮动静态遮挡物**：如右下角“回到顶部”悬浮球或局部提示窗，在活跃正文区间内部
//!    呈小块静止状态，但与按全局位移对齐后的画面严重互斥；`heal_canvas_with_floating_mask`
//!    利用下一帧刚滑出浮层阴影的干净重叠区将此前压入画布的浮层残影无损擦除恢复正文。

use super::matcher::rgb_to_luma;
use super::types::StitchDirection;
use image::RgbaImage;

/// 检测出的首尾固定栏像素厚度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct StickyBands {
    pub(crate) header: usize,
    pub(crate) footer: usize,
}

/// 探测连续两帧头部与尾部固定不动且含真实 UI 结构的通栏厚度。
pub(crate) fn detect_sticky_bands(
    prev: &[u8],
    curr: &[u8],
    width: usize,
    height: usize,
    max_ratio: f32,
    x_start: usize,
    x_end: usize,
) -> StickyBands {
    let max_band = ((height as f32) * max_ratio).clamp(0.0, height as f32) as usize;
    let header = scan_sticky_header(prev, curr, width, max_band, x_start, x_end);

    // 为匹配留出最小正文窗体，禁止尾栏探测侵入头栏或压缩活跃高度至不足 32px。
    let max_footer = max_band.min(height.saturating_sub(header).saturating_sub(32));
    let footer = scan_sticky_footer(prev, curr, width, height, max_footer, x_start, x_end);

    StickyBands { header, footer }
}

fn scan_sticky_header(
    prev: &[u8],
    curr: &[u8],
    width: usize,
    max_rows: usize,
    x_start: usize,
    x_end: usize,
) -> usize {
    let mut rows = 0usize;
    for y in 0..max_rows {
        if row_avg_diff(prev, curr, y, width, x_start, x_end) <= 3.5 {
            rows += 1;
        } else {
            break;
        }
    }
    if rows == 0 || !band_has_horizontal_texture(curr, 0, rows, width, x_start, x_end) {
        return 0;
    }
    rows
}

fn scan_sticky_footer(
    prev: &[u8],
    curr: &[u8],
    width: usize,
    height: usize,
    max_rows: usize,
    x_start: usize,
    x_end: usize,
) -> usize {
    let mut rows = 0usize;
    for offset in 0..max_rows {
        let y = height - 1 - offset;
        if row_avg_diff(prev, curr, y, width, x_start, x_end) <= 3.5 {
            rows += 1;
        } else {
            break;
        }
    }
    if rows < 4 {
        return 0;
    }
    let y_start = height - rows;
    if !band_has_2d_ui_texture(curr, y_start, height, width, x_start, x_end) {
        return 0;
    }
    rows
}

fn row_avg_diff(
    prev: &[u8],
    curr: &[u8],
    y: usize,
    width: usize,
    x_start: usize,
    x_end: usize,
) -> f64 {
    let base = y * width;
    let row_len = (x_end - x_start).max(1);
    let sum: u64 = (x_start..x_end)
        .map(|x| u64::from(prev[base + x].abs_diff(curr[base + x])))
        .sum();
    (sum as f64) / (row_len as f64)
}

fn band_has_horizontal_texture(
    plane: &[u8],
    y_start: usize,
    y_end: usize,
    width: usize,
    x_start: usize,
    x_end: usize,
) -> bool {
    let row_len = x_end.saturating_sub(x_start);
    if row_len < 2 || y_start >= y_end {
        return false;
    }
    for y in y_start..y_end {
        let base = y * width;
        let mut min_v = u8::MAX;
        let mut max_v = u8::MIN;
        let mut grad_sum = 0u64;
        for x in x_start..x_end {
            let v = plane[base + x];
            min_v = min_v.min(v);
            max_v = max_v.max(v);
            if x > x_start {
                grad_sum += u64::from(v.abs_diff(plane[base + x - 1]));
            }
        }
        if max_v.saturating_sub(min_v) >= 8 || (grad_sum as f64 / ((row_len - 1) as f64)) > 1.2 {
            return true;
        }
    }
    false
}

/// 要求候选页脚同时具备横向细节与纵向结构变化，防止垂直无变化的纯条纹或纯白边距误当固定底栏剥离。
fn band_has_2d_ui_texture(
    plane: &[u8],
    y_start: usize,
    y_end: usize,
    width: usize,
    x_start: usize,
    x_end: usize,
) -> bool {
    if !band_has_horizontal_texture(plane, y_start, y_end, width, x_start, x_end) {
        return false;
    }
    let row_len = x_end.saturating_sub(x_start).max(1);
    let mut min_row_mean = f64::MAX;
    let mut max_row_mean = f64::MIN;
    let mut max_adj_v_diff = 0.0f64;

    for y in y_start..y_end {
        let base = y * width;
        let sum: u64 = (x_start..x_end).map(|x| u64::from(plane[base + x])).sum();
        let mean = (sum as f64) / (row_len as f64);
        min_row_mean = min_row_mean.min(mean);
        max_row_mean = max_row_mean.max(mean);

        if y > y_start {
            let prev_base = (y - 1) * width;
            let v_sum: u64 = (x_start..x_end)
                .map(|x| u64::from(plane[base + x].abs_diff(plane[prev_base + x])))
                .sum();
            max_adj_v_diff = max_adj_v_diff.max((v_sum as f64) / (row_len as f64));
        }
    }
    (max_row_mean - min_row_mean) >= 4.0 || max_adj_v_diff >= 3.0
}

/// 浮动前景遮挡自愈输入上下文。
pub(crate) struct OverlayHealSpec<'a> {
    pub(crate) prev: &'a RgbaImage,
    pub(crate) curr: &'a RgbaImage,
    pub(crate) direction: StitchDirection,
    pub(crate) step: u32,
    pub(crate) header: u32,
    pub(crate) footer: u32,
    pub(crate) existing_canvas_header: u32,
    pub(crate) existing_canvas_footer: u32,
    /// `false`: 向尾部追加（offset > 0）；`true`: 向首部前插（offset < 0）
    pub(crate) is_prepend: bool,
}

/// 识别静止不动的局部浮动浮层（如右下角悬浮按钮），并用当前帧刚刚滑出遮挡区的新露像素覆盖抹平旧残影。
pub(crate) fn heal_canvas_with_floating_mask(canvas: &mut RgbaImage, spec: &OverlayHealSpec<'_>) {
    let (frame_main, cross_span) = axis_dims(spec.curr, spec.direction);
    let canvas_main = axis_dims(canvas, spec.direction).0;
    let body_end = frame_main.saturating_sub(spec.footer);
    let body_start = spec.header.min(body_end);
    if spec.step == 0 || body_end <= body_start + spec.step {
        return;
    }

    let block_m = 4u32;
    let block_c = 8u32;
    let cols = cross_span.div_ceil(block_c) as usize;
    let rows = (body_end - body_start).div_ceil(block_m) as usize;
    if cols == 0 || rows == 0 {
        return;
    }

    let Some(dilated) =
        build_floating_mask(spec, body_start, body_end, (cols, rows), (block_m, block_c))
    else {
        return;
    };

    apply_clean_backfill(
        canvas,
        spec,
        &dilated,
        BackfillGrid {
            body_start,
            body_end,
            canvas_body_end: canvas_main.saturating_sub(spec.existing_canvas_footer),
            block_m,
            block_c,
            cols,
            rows,
            cross_span,
        },
    );
}

fn build_floating_mask(
    spec: &OverlayHealSpec<'_>,
    body_start: u32,
    body_end: u32,
    (cols, rows): (usize, usize),
    (block_m, block_c): (u32, u32),
) -> Option<Vec<bool>> {
    let cross_span = axis_dims(spec.curr, spec.direction).1;
    let mut mask = vec![false; cols * rows];
    let mut occluded_count = 0usize;
    for r in 0..rows {
        let m0 = body_start + (r as u32) * block_m;
        let m1 = (m0 + block_m).min(body_end);
        for c in 0..cols {
            let c0 = (c as u32) * block_c;
            let c1 = (c0 + block_c).min(cross_span);
            if is_floating_block(spec, (m0, m1), (c0, c1)) {
                mask[r * cols + c] = true;
                occluded_count += 1;
            }
        }
    }

    // 仅当浮层占活跃区比例合理（<= 30%）时才执行干净重叠回填，防止静态纯色区误触发覆写。
    if occluded_count == 0 || occluded_count.saturating_mul(10) > mask.len().saturating_mul(3) {
        return None;
    }

    // 横向扩张 1 格覆盖抗锯齿边缘光晕。
    let mut dilated = mask.clone();
    for r in 0..rows {
        for c in 0..cols {
            if mask[r * cols + c] {
                if c > 0 {
                    dilated[r * cols + c - 1] = true;
                }
                if c + 1 < cols {
                    dilated[r * cols + c + 1] = true;
                }
            }
        }
    }
    Some(dilated)
}

fn is_floating_block(spec: &OverlayHealSpec<'_>, m_range: (u32, u32), c_range: (u32, u32)) -> bool {
    let mut static_sum = 0u64;
    let mut shifted_sum = 0u64;
    let mut shifted_count = 0u64;
    let mut count = 0u64;
    let frame_main = axis_dims(spec.curr, spec.direction).0;
    let body_end = frame_main.saturating_sub(spec.footer);
    let body_start = spec.header.min(body_end);

    for m in m_range.0..m_range.1 {
        for c in c_range.0..c_range.1 {
            let p = read_axis_pixel(spec.prev, m, c, spec.direction);
            let q = read_axis_pixel(spec.curr, m, c, spec.direction);
            let pl = rgb_to_luma(p[0], p[1], p[2]);
            let ql = rgb_to_luma(q[0], q[1], q[2]);
            static_sum += u64::from(pl.abs_diff(ql));
            count += 1;

            // 按滚动极性确定相邻帧的对齐偏移符号，校验静态浮层与真实位移正文间的显著反差。
            if !spec.is_prepend {
                if m + spec.step < body_end {
                    let s_px = read_axis_pixel(spec.prev, m + spec.step, c, spec.direction);
                    let sl = rgb_to_luma(s_px[0], s_px[1], s_px[2]);
                    shifted_sum += u64::from(ql.abs_diff(sl));
                    shifted_count += 1;
                }
                if m >= body_start + spec.step {
                    let c_px = read_axis_pixel(spec.curr, m - spec.step, c, spec.direction);
                    let cl = rgb_to_luma(c_px[0], c_px[1], c_px[2]);
                    shifted_sum += u64::from(pl.abs_diff(cl));
                    shifted_count += 1;
                }
            } else {
                if m + spec.step < body_end {
                    let c_px = read_axis_pixel(spec.curr, m + spec.step, c, spec.direction);
                    let cl = rgb_to_luma(c_px[0], c_px[1], c_px[2]);
                    shifted_sum += u64::from(pl.abs_diff(cl));
                    shifted_count += 1;
                }
                if m >= body_start + spec.step {
                    let s_px = read_axis_pixel(spec.prev, m - spec.step, c, spec.direction);
                    let sl = rgb_to_luma(s_px[0], s_px[1], s_px[2]);
                    shifted_sum += u64::from(ql.abs_diff(sl));
                    shifted_count += 1;
                }
            }
        }
    }

    if count == 0 || shifted_count == 0 {
        return false;
    }
    let static_avg = (static_sum as f64) / (count as f64);
    let shifted_avg = (shifted_sum as f64) / (shifted_count as f64);
    static_avg <= 6.0 && shifted_avg >= 12.0
}

struct BackfillGrid {
    body_start: u32,
    body_end: u32,
    canvas_body_end: u32,
    block_m: u32,
    block_c: u32,
    cols: usize,
    rows: usize,
    cross_span: u32,
}

fn apply_clean_backfill(
    canvas: &mut RgbaImage,
    spec: &OverlayHealSpec<'_>,
    mask: &[bool],
    grid: BackfillGrid,
) {
    let canvas_main = axis_dims(canvas, spec.direction).0;
    let range_start = if spec.is_prepend {
        grid.body_start + spec.step
    } else {
        grid.body_start
    };
    let range_end = if spec.is_prepend {
        grid.body_end
    } else {
        grid.body_end.saturating_sub(spec.step)
    };
    for curr_m in range_start..range_end {
        let r_curr = ((curr_m - grid.body_start) / grid.block_m) as usize;
        let prev_m = if spec.is_prepend {
            curr_m - spec.step
        } else {
            curr_m + spec.step
        };
        let r_prev = ((prev_m - grid.body_start) / grid.block_m) as usize;
        if r_curr >= grid.rows || r_prev >= grid.rows {
            continue;
        }
        let canvas_m = if spec.is_prepend {
            let rel_in_body = prev_m.saturating_sub(grid.body_start);
            let target = spec.existing_canvas_header.saturating_add(rel_in_body);
            if target >= canvas_main {
                continue;
            }
            target
        } else {
            let dist_from_body_end = grid.body_end.saturating_sub(prev_m);
            if dist_from_body_end > grid.canvas_body_end {
                continue;
            }
            let target = grid.canvas_body_end - dist_from_body_end;
            if target >= grid.canvas_body_end {
                continue;
            }
            target
        };

        for c_idx in 0..grid.cols {
            let curr_occluded = mask[r_curr * grid.cols + c_idx];
            let prev_was_occluded = mask[r_prev * grid.cols + c_idx];
            if !curr_occluded && prev_was_occluded {
                let c0 = (c_idx as u32) * grid.block_c;
                let c1 = (c0 + grid.block_c).min(grid.cross_span);
                for c in c0..c1 {
                    let clean_px = read_axis_pixel(spec.curr, curr_m, c, spec.direction);
                    write_axis_pixel(canvas, canvas_m, c, spec.direction, clean_px);
                }
            }
        }
    }
}

fn axis_dims(img: &RgbaImage, direction: StitchDirection) -> (u32, u32) {
    match direction {
        StitchDirection::Vertical => (img.height(), img.width()),
        StitchDirection::Horizontal => (img.width(), img.height()),
    }
}

fn read_axis_pixel(img: &RgbaImage, main: u32, cross: u32, direction: StitchDirection) -> [u8; 4] {
    let (x, y) = match direction {
        StitchDirection::Vertical => (cross, main),
        StitchDirection::Horizontal => (main, cross),
    };
    img.get_pixel(x, y).0
}

fn write_axis_pixel(
    img: &mut RgbaImage,
    main: u32,
    cross: u32,
    direction: StitchDirection,
    rgba: [u8; 4],
) {
    let (x, y) = match direction {
        StitchDirection::Vertical => (cross, main),
        StitchDirection::Horizontal => (main, cross),
    };
    img.put_pixel(x, y, image::Rgba(rgba));
}
