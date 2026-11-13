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
use super::paged_canvas::{put_axis_pixel, PagedCanvas};
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

/// 允许桥接固定栏内部完全波动（如动态分割线、光标闪烁）的最大连续行数。
const MAX_PURE_JITTER_ROWS: usize = 2;
/// 允许桥接固定栏内部局部小控件（如时钟、角标）波动的最大连续行数。
const MAX_LOCAL_INDICATOR_ROWS: usize = 12;
/// 桥接局部动态控件后，要求恢复的静态且带纹理边框段的最少连续行数。
const MIN_RESUME_STATIC_ROWS: usize = 3;
/// 单条固定栏内部累计允许豁免的动态行数上限，防止跨多个内容块误桥接。
const MAX_TOTAL_BRIDGED_ROWS: usize = 16;
/// 固定栏内侧边缘允许保留的纯平无结构内边距最大行数，超出则判定为正文纯色留白并回缩。
const MAX_TRAILING_FLAT_ROWS: usize = 8;

fn scan_sticky_header(
    prev: &[u8],
    curr: &[u8],
    width: usize,
    max_rows: usize,
    x_start: usize,
    x_end: usize,
) -> usize {
    let rows = scan_sticky_edge_rows(
        &StickyScanView {
            prev,
            curr,
            width,
            height: max_rows,
            x_start,
            x_end,
        },
        max_rows,
        false,
    );
    if rows == 0 {
        return 0;
    }
    let band = BandRegion {
        y_start: 0,
        y_end: rows,
        width,
        x_start,
        x_end,
    };
    if !band_has_horizontal_texture(curr, band) && !band_has_vertical_structure(curr, band) {
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
    let rows = scan_sticky_edge_rows(
        &StickyScanView {
            prev,
            curr,
            width,
            height,
            x_start,
            x_end,
        },
        max_rows,
        true,
    );
    if rows < 4 {
        return 0;
    }
    let y_start = height - rows;
    let band = BandRegion {
        y_start,
        y_end: height,
        width,
        x_start,
        x_end,
    };
    if !band_has_2d_ui_texture(curr, band) {
        return 0;
    }
    rows
}

struct StickyScanView<'a> {
    prev: &'a [u8],
    curr: &'a [u8],
    width: usize,
    height: usize,
    x_start: usize,
    x_end: usize,
}

impl StickyScanView<'_> {
    fn map_y(&self, step: usize, from_bottom: bool) -> usize {
        if from_bottom {
            self.height - 1 - step
        } else {
            step
        }
    }

    fn is_strict_static(&self, y: usize) -> bool {
        row_avg_diff(
            self.prev,
            self.curr,
            y,
            self.width,
            self.x_start,
            self.x_end,
        ) <= 3.5
    }

    fn is_mostly_static(&self, y: usize) -> bool {
        row_unchanged_ratio(
            self.prev,
            self.curr,
            y,
            self.width,
            self.x_start,
            self.x_end,
        ) >= 0.90
    }
}

/// 扫描固定栏（顶栏或底栏），在确认已进入静态栏后容忍内部局部动态指示器（如时钟、角标）。
fn scan_sticky_edge_rows(view: &StickyScanView<'_>, max_rows: usize, from_bottom: bool) -> usize {
    let mut rows = 0usize;
    while rows < max_rows && view.is_strict_static(view.map_y(rows, from_bottom)) {
        rows += 1;
    }
    if rows < 2 {
        return rows;
    }

    let mut bridged_total = 0usize;
    while rows < max_rows && bridged_total < MAX_TOTAL_BRIDGED_ROWS {
        let Some((resume_end, gap_len)) =
            try_bridge_dynamic_gap(view, rows, max_rows, from_bottom, bridged_total)
        else {
            break;
        };
        bridged_total += gap_len;
        rows = resume_end;
    }
    trim_trailing_uniform_body_margin(view, rows, from_bottom)
}

/// 回缩固定栏内侧末尾的大段纯平区域，避免将紧邻顶/底栏的纯色正文留白吞入固定栏。
fn trim_trailing_uniform_body_margin(
    view: &StickyScanView<'_>,
    rows: usize,
    from_bottom: bool,
) -> usize {
    if rows <= MAX_TRAILING_FLAT_ROWS {
        return rows;
    }
    let mut step = rows;
    while step > 0 {
        let cur_idx = step - 1;
        let y = view.map_y(cur_idx, from_bottom);
        let has_h_tex =
            row_has_horizontal_texture(view.curr, y, view.width, view.x_start, view.x_end);
        // 与更内侧一行对比垂直跳变，精确停在固定栏背景边界或分割线本体，不额外吞入内侧纯白首行
        let has_v_edge = step < rows
            && row_avg_diff_same_plane(
                view.curr,
                y,
                view.map_y(step, from_bottom),
                view.width,
                view.x_start,
                view.x_end,
            ) >= 2.0;
        if has_h_tex || has_v_edge {
            let trailing_flat = rows - step;
            return if trailing_flat > MAX_TRAILING_FLAT_ROWS {
                step
            } else {
                rows
            };
        }
        step -= 1;
    }
    rows
}

fn try_bridge_dynamic_gap(
    view: &StickyScanView<'_>,
    start_step: usize,
    max_rows: usize,
    from_bottom: bool,
    bridged_total: usize,
) -> Option<(usize, usize)> {
    let remaining_budget = MAX_TOTAL_BRIDGED_ROWS.saturating_sub(bridged_total);
    let max_gap = MAX_LOCAL_INDICATOR_ROWS.min(remaining_budget);
    let mut probe = start_step;
    let mut gap_len = 0usize;

    while probe < max_rows {
        let y = view.map_y(probe, from_bottom);
        if view.is_strict_static(y) {
            break;
        }
        gap_len += 1;
        if gap_len > max_gap {
            return None;
        }
        if gap_len > MAX_PURE_JITTER_ROWS && !view.is_mostly_static(y) {
            return None;
        }
        probe += 1;
    }

    if gap_len == 0 || probe >= max_rows {
        return None;
    }

    let resume_start = probe;
    let mut resume_end = resume_start;
    while resume_end < max_rows && view.is_strict_static(view.map_y(resume_end, from_bottom)) {
        resume_end += 1;
    }
    if resume_end - resume_start < MIN_RESUME_STATIC_ROWS {
        return None;
    }

    let (y_start, y_end) = if from_bottom {
        (view.height - resume_end, view.height - resume_start)
    } else {
        (resume_start, resume_end)
    };
    let resume_band = BandRegion {
        y_start,
        y_end,
        width: view.width,
        x_start: view.x_start,
        x_end: view.x_end,
    };
    if !band_has_horizontal_texture(view.curr, resume_band)
        && !band_has_vertical_structure(view.curr, resume_band)
    {
        return None;
    }

    Some((resume_end, gap_len))
}

fn row_unchanged_ratio(
    prev: &[u8],
    curr: &[u8],
    y: usize,
    width: usize,
    x_start: usize,
    x_end: usize,
) -> f64 {
    let base = y * width;
    let row_len = (x_end - x_start).max(1);
    let unchanged = (x_start..x_end)
        .filter(|&x| prev[base + x].abs_diff(curr[base + x]) <= 3)
        .count();
    (unchanged as f64) / (row_len as f64)
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

fn row_avg_diff_same_plane(
    plane: &[u8],
    y_a: usize,
    y_b: usize,
    width: usize,
    x_start: usize,
    x_end: usize,
) -> f64 {
    let base_a = y_a * width;
    let base_b = y_b * width;
    let row_len = (x_end - x_start).max(1);
    let sum: u64 = (x_start..x_end)
        .map(|x| u64::from(plane[base_a + x].abs_diff(plane[base_b + x])))
        .sum();
    (sum as f64) / (row_len as f64)
}

#[derive(Clone, Copy)]
struct BandRegion {
    y_start: usize,
    y_end: usize,
    width: usize,
    x_start: usize,
    x_end: usize,
}

fn row_has_horizontal_texture(
    plane: &[u8],
    y: usize,
    width: usize,
    x_start: usize,
    x_end: usize,
) -> bool {
    let row_len = x_end.saturating_sub(x_start);
    if row_len < 2 {
        return false;
    }
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
    max_v.saturating_sub(min_v) >= 5 || (grad_sum as f64 / ((row_len - 1) as f64)) > 0.6
}

fn band_has_horizontal_texture(plane: &[u8], band: BandRegion) -> bool {
    if band.y_start >= band.y_end {
        return false;
    }
    (band.y_start..band.y_end)
        .any(|y| row_has_horizontal_texture(plane, y, band.width, band.x_start, band.x_end))
}

/// 检测候选栏是否存在跨行均值差异或静态水平分割线，兼顾极简纯色底栏与带边线顶栏。
fn band_has_vertical_structure(plane: &[u8], band: BandRegion) -> bool {
    let row_len = band.x_end.saturating_sub(band.x_start).max(1);
    if band.y_end.saturating_sub(band.y_start) < 2 {
        return false;
    }
    let mut min_row_mean = f64::MAX;
    let mut max_row_mean = f64::MIN;
    let mut max_adj_v_diff = 0.0f64;

    for y in band.y_start..band.y_end {
        let base = y * band.width;
        let sum: u64 = (band.x_start..band.x_end)
            .map(|x| u64::from(plane[base + x]))
            .sum();
        let mean = (sum as f64) / (row_len as f64);
        min_row_mean = min_row_mean.min(mean);
        max_row_mean = max_row_mean.max(mean);

        if y > band.y_start {
            let prev_base = (y - 1) * band.width;
            let v_sum: u64 = (band.x_start..band.x_end)
                .map(|x| u64::from(plane[base + x].abs_diff(plane[prev_base + x])))
                .sum();
            max_adj_v_diff = max_adj_v_diff.max((v_sum as f64) / (row_len as f64));
        }
    }
    (max_row_mean - min_row_mean) >= 3.0 || max_adj_v_diff >= 3.0
}

/// 要求候选页脚具备纵向结构变化（防止纯垂直条纹误检），同时兼容极简纯色+分割线或低对比度 UI 图标。
fn band_has_2d_ui_texture(plane: &[u8], band: BandRegion) -> bool {
    if !band_has_vertical_structure(plane, band) {
        return false;
    }
    band_has_horizontal_texture(plane, band) || band.y_end.saturating_sub(band.y_start) >= 4
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
pub(crate) fn heal_canvas_with_floating_mask(canvas: &mut PagedCanvas, spec: &OverlayHealSpec<'_>) {
    let (frame_main, cross_span) = axis_dims(spec.curr, spec.direction);
    let canvas_main = canvas.main_span();
    let body_end = frame_main.saturating_sub(spec.footer.max(spec.existing_canvas_footer));
    let body_start = spec.header.max(spec.existing_canvas_header).min(body_end);
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

    // 3x3 邻域膨胀 1 格，覆盖浮层四向抗锯齿边缘与投影光晕。
    let mut dilated = mask.clone();
    for r in 0..rows {
        for c in 0..cols {
            if !mask[r * cols + c] {
                continue;
            }
            for nr in r.saturating_sub(1)..=(r + 1).min(rows - 1) {
                for nc in c.saturating_sub(1)..=(c + 1).min(cols - 1) {
                    dilated[nr * cols + nc] = true;
                }
            }
        }
    }
    Some(dilated)
}

#[inline]
fn pixel_visual_diff(a: [u8; 4], b: [u8; 4]) -> u64 {
    let luma_diff =
        u64::from(rgb_to_luma(a[0], a[1], a[2]).abs_diff(rgb_to_luma(b[0], b[1], b[2])));
    let dr = u64::from(a[0].abs_diff(b[0]));
    let dg = u64::from(a[1].abs_diff(b[1]));
    let db = u64::from(a[2].abs_diff(b[2]));
    luma_diff.max((dr + dg + db) / 3)
}

fn is_floating_block(spec: &OverlayHealSpec<'_>, m_range: (u32, u32), c_range: (u32, u32)) -> bool {
    let mut static_sum = 0u64;
    let mut shifted_sum = 0u64;
    let mut shifted_count = 0u64;
    let mut count = 0u64;
    let frame_main = axis_dims(spec.curr, spec.direction).0;
    let body_end = frame_main.saturating_sub(spec.footer.max(spec.existing_canvas_footer));
    let body_start = spec.header.max(spec.existing_canvas_header).min(body_end);

    for m in m_range.0..m_range.1 {
        for c in c_range.0..c_range.1 {
            let p = read_axis_pixel(spec.prev, m, c, spec.direction);
            let q = read_axis_pixel(spec.curr, m, c, spec.direction);
            static_sum += pixel_visual_diff(p, q);
            count += 1;

            // 按滚动极性确定相邻帧的对齐偏移符号，校验静态浮层与真实位移正文间的显著反差。
            if !spec.is_prepend {
                if m + spec.step < body_end {
                    let s_px = read_axis_pixel(spec.prev, m + spec.step, c, spec.direction);
                    shifted_sum += pixel_visual_diff(q, s_px);
                    shifted_count += 1;
                }
                if m >= body_start + spec.step {
                    let c_px = read_axis_pixel(spec.curr, m - spec.step, c, spec.direction);
                    shifted_sum += pixel_visual_diff(p, c_px);
                    shifted_count += 1;
                }
            } else {
                if m + spec.step < body_end {
                    let c_px = read_axis_pixel(spec.curr, m + spec.step, c, spec.direction);
                    shifted_sum += pixel_visual_diff(p, c_px);
                    shifted_count += 1;
                }
                if m >= body_start + spec.step {
                    let s_px = read_axis_pixel(spec.prev, m - spec.step, c, spec.direction);
                    shifted_sum += pixel_visual_diff(q, s_px);
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
    canvas: &mut PagedCanvas,
    spec: &OverlayHealSpec<'_>,
    mask: &[bool],
    grid: BackfillGrid,
) {
    let canvas_main = canvas.main_span();
    let canvas_cross = match spec.direction {
        StitchDirection::Vertical => canvas.dimensions().0,
        StitchDirection::Horizontal => canvas.dimensions().1,
    };
    let cross_limit = grid.cross_span.min(canvas_cross);
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
    let frame_main = axis_dims(spec.curr, spec.direction).0;
    let prev_canvas_body_start = spec.existing_canvas_header;
    let prev_canvas_body_end = frame_main.saturating_sub(spec.existing_canvas_footer);
    let mut cursor = canvas.write_cursor();
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
            if prev_m < prev_canvas_body_start {
                continue;
            }
            let rel_in_body = prev_m - prev_canvas_body_start;
            let target = spec.existing_canvas_header.saturating_add(rel_in_body);
            if target >= canvas_main {
                continue;
            }
            target
        } else {
            if prev_m >= prev_canvas_body_end {
                continue;
            }
            let dist_from_body_end = prev_canvas_body_end - prev_m;
            if dist_from_body_end > grid.canvas_body_end {
                continue;
            }
            let target = grid.canvas_body_end - dist_from_body_end;
            if target >= grid.canvas_body_end {
                continue;
            }
            target
        };

        let mut row_target = None;
        for c_idx in 0..grid.cols {
            let curr_occluded = mask[r_curr * grid.cols + c_idx];
            let prev_was_occluded = mask[r_prev * grid.cols + c_idx];
            if !curr_occluded && prev_was_occluded {
                if row_target.is_none() {
                    row_target = cursor.locate_row_mut(canvas_m);
                }
                let Some((ref mut chunk, local_m, dir)) = row_target else {
                    break;
                };
                let c0 = (c_idx as u32) * grid.block_c;
                let c1 = (c0 + grid.block_c).min(cross_limit);
                for c in c0..c1 {
                    let clean_px = read_axis_pixel(spec.curr, curr_m, c, spec.direction);
                    put_axis_pixel(chunk, local_m, c, dir, clean_px);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimalist_solid_header_with_divider_detected_while_uniform_margin_rejected() {
        let width = 120usize;
        let height = 120usize;
        let mut prev = vec![250u8; width * height];
        let mut curr = vec![250u8; width * height];

        // 顶部 18px 极简纯白顶栏 + 第 18px 静态 1px 灰色分割线，紧接着下方为滚动正文纹理。
        for x in 0..width {
            prev[18 * width + x] = 225;
            curr[18 * width + x] = 225;
        }
        for y in 19..height {
            for x in 0..width {
                prev[y * width + x] = ((y * 17 + x * 5) % 200) as u8;
                curr[y * width + x] = (((y + 20) * 17 + x * 5) % 200) as u8;
            }
        }

        let bands = detect_sticky_bands(&prev, &curr, width, height, 0.35, 12, 108);
        assert_eq!(bands.header, 19, "带静态 1px 分割线的极简纯色顶栏应被识别");
        assert_eq!(bands.footer, 0);

        // 纯白无分割线、无图标的空白边距不得被误识别为固定顶栏。
        let mut uniform_prev = vec![250u8; width * height];
        let mut uniform_curr = vec![250u8; width * height];
        for y in 19..height {
            for x in 0..width {
                uniform_prev[y * width + x] = ((y * 17 + x * 5) % 200) as u8;
                uniform_curr[y * width + x] = (((y + 20) * 17 + x * 5) % 200) as u8;
            }
        }
        let uniform_bands =
            detect_sticky_bands(&uniform_prev, &uniform_curr, width, height, 0.35, 12, 108);
        assert_eq!(uniform_bands.header, 0, "纯白均匀边距不应误判为固定顶栏");
    }

    #[test]
    fn test_sticky_bars_tolerate_localized_dynamic_clock_without_swallowing_sparse_body() {
        let width = 200usize;
        let height = 220usize;
        let mut prev = vec![248u8; width * height];
        let mut curr = vec![248u8; width * height];

        // 顶部 32px 顶栏：y=0..8 为静态标题带，y=8..16 存在 10px 宽动态闪烁角标，y=16..31 为静态下衬与第 31 行分割线
        for x in 20..120 {
            for y in 2..7 {
                prev[y * width + x] = 80;
                curr[y * width + x] = 80;
            }
        }
        for y in 8..16 {
            for x in 170..180 {
                prev[y * width + x] = 40;
                curr[y * width + x] = 240;
            }
        }
        for x in 0..width {
            prev[31 * width + x] = 210;
            curr[31 * width + x] = 210;
        }

        // 底部 26px 底栏：y=height-26 为顶分割线，y=height-14..height-8 存在局部动态指示灯
        for x in 0..width {
            prev[(height - 26) * width + x] = 205;
            curr[(height - 26) * width + x] = 205;
        }
        for y in (height - 14)..(height - 8) {
            for x in 30..38 {
                prev[y * width + x] = 30;
                curr[y * width + x] = 220;
            }
        }

        // 正文区 y=32..height-26：y=32..40 故意放置窄列文本后接纯白行距 (y=40..50)，验证不会被误桥接吞入顶栏
        for y in 32..40 {
            for x in 24..34 {
                prev[y * width + x] = 35;
            }
        }
        for y in 50..(height - 26) {
            for x in 16..184 {
                prev[y * width + x] = ((y * 19 + x * 7) % 190 + 30) as u8;
                curr[y * width + x] = (((y + 24) * 19 + x * 7) % 190 + 30) as u8;
            }
        }

        let bands = detect_sticky_bands(&prev, &curr, width, height, 0.35, 16, 184);
        assert_eq!(
            bands.header, 32,
            "含内部局部动态角标的固定顶栏应完整识别到分割线"
        );
        assert_eq!(
            bands.footer, 26,
            "含内部局部动态指示灯的固定底栏应完整识别到顶分割线"
        );
    }

    #[test]
    fn test_sticky_bars_do_not_swallow_adjacent_uniform_body_whitespace() {
        let width = 160usize;
        let height = 220usize;
        let mut prev = vec![250u8; width * height];
        let mut curr = vec![250u8; width * height];

        // 顶部 20px 固定栏 + 第 20 行分割线，紧接着 25 行完全纯白正文留白 (y=21..46)
        for x in 0..width {
            prev[20 * width + x] = 210;
            curr[20 * width + x] = 210;
        }
        // 底部 24px 固定栏 + y=height-24 分割线，上方紧邻 25 行纯白正文留白 (height-49..height-24)
        for x in 0..width {
            prev[(height - 24) * width + x] = 205;
            curr[(height - 24) * width + x] = 205;
        }
        // 中间真实滚动纹理区 (y=46..height-49)
        for y in 46..(height - 49) {
            for x in 10..150 {
                prev[y * width + x] = ((y * 13 + x * 7) % 180 + 40) as u8;
                curr[y * width + x] = (((y + 18) * 13 + x * 7) % 180 + 40) as u8;
            }
        }

        let bands = detect_sticky_bands(&prev, &curr, width, height, 0.40, 10, 150);
        assert_eq!(bands.header, 21, "顶栏不应吞入紧邻的纯白正文留白");
        assert_eq!(bands.footer, 24, "底栏不应吞入紧邻的纯白正文留白");
    }

    #[test]
    fn test_heal_floating_overlay_handles_isoluminant_badge_vertical_halo_and_footer_mismatch() {
        use image::Rgba;
        let width = 120u32;
        let height = 200u32;
        let step = 40u32;
        // 构造同亮度但彩色对比明显的浮动徽章 (y=80..96, x=88..104) 以及上下 2px 淡灰投影光晕 (y=78..80, 96..98)
        let build_frame = |scroll: u32, footer_h: u32| -> RgbaImage {
            let mut img = RgbaImage::new(width, height);
            for y in 0..height {
                for x in 0..width {
                    if y >= height - footer_h {
                        img.put_pixel(x, y, Rgba([20, 20, 20, 255]));
                        continue;
                    }
                    let doc_y = y + scroll;
                    // 等亮度蓝色纹理正文 (luma ~ 128)
                    let base: [u8; 4] = if (doc_y / 8 + x / 16).is_multiple_of(2) {
                        [100, 130, 190, 255]
                    } else {
                        [130, 128, 128, 255]
                    };
                    let mut px = base;
                    // 同亮度红色浮动角标及上下阴影光晕
                    if (88..104).contains(&x) && (80..96).contains(&y) {
                        px = [210, 110, 90, 255];
                    } else if (88..104).contains(&x)
                        && ((78..80).contains(&y) || (96..98).contains(&y))
                    {
                        // 轻微阴影（差值不足 12），依赖 3x3 膨胀覆盖
                        px = [
                            px[0].saturating_sub(8),
                            px[1].saturating_sub(8),
                            px[2].saturating_sub(8),
                            255,
                        ];
                    }
                    img.put_pixel(x, y, Rgba(px));
                }
            }
            img
        };

        let f0 = build_frame(0, 20);
        let f1 = build_frame(step, 32);
        let mut canvas = PagedCanvas::new(f0.clone(), StitchDirection::Vertical);
        let spec = OverlayHealSpec {
            prev: &f0,
            curr: &f1,
            direction: StitchDirection::Vertical,
            step,
            header: 0,
            footer: 32,
            existing_canvas_header: 0,
            existing_canvas_footer: 20,
            is_prepend: false,
        };
        heal_canvas_with_floating_mask(&mut canvas, &spec);

        let healed = canvas.as_contiguous();
        // 原来位于 y=80..96 以及上下阴影 y=79、y=96 的浮动角标和光晕应被正确坐标的正文像素擦除恢复
        for y in [79u32, 82, 88, 95, 96] {
            let expected = if (y / 8 + 94 / 16).is_multiple_of(2) {
                [100, 130, 190, 255]
            } else {
                [130, 128, 128, 255]
            };
            assert_eq!(
                healed.get_pixel(94, y).0,
                expected,
                "第 {y} 行浮层或纵向光晕未按正确正文坐标无损自愈"
            );
        }
    }
}
