//! 2 级由粗到细金字塔特征构造、多峰粗定位与多频带 ZNCC 结构一致性校验。
//!
//! # 设计原理
//!
//! 单一尺度的全分辨率逐像素滑动窗口匹配存在两大瓶颈：
//! 1. **计算复杂度高**：高分屏下允许最大步长可达上千像素，单帧暴力全分辨率搜索可达
//!    数千万次像素比较，极易引发卡顿与掉帧；先通过 `4×4` 盒式均值构造兼顾亮度和色差对立
//!    通道的粗尺度金字塔筛选出前若干个局部极小波谷候选，再回到原图精细邻域 (`±4px`) 内精定位，
//!    可在保持像素级精度的前提下将大位移搜索耗时压低一个数量级以上；同时保留微步长区间直接细搜，
//!    避免 `2~10px` 微滚动被降采样模糊淹没。
//! 2. **单一亮度通道脆弱性**：Rec.601 灰度会将同亮度的不同彩色徽标或语法高亮映射为相同数值，
//!    且轻微全局亮度漂移会污染纯 SAD 打分。通过 `[Luma, R-G, G-B]` 三正交基与最大通道梯度构图，
//!    再在最末阶段对重叠区拆分多频带计算零均值归一化互相关（ZNCC）结构共识，可同时免疫
//!    同亮度色块失配、轻微曝光变化及半屏局部突变导致的错拼。

/// 单个像素的三通道正交特征：`[Rec.601亮度, R-G对立色差, G-B对立色差]`。
pub(crate) type FeatureRgb = [u8; 3];

/// 供局部匹配器共用的二维三通道特征平面与沿主轴梯度图。
pub(crate) struct FeaturePlane {
    pub(crate) pixels: Vec<FeatureRgb>,
    pub(crate) grad: Vec<u8>,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

impl FeaturePlane {
    /// 将原始纵向 RGBA 像素转换为三通道特征与沿 Y 轴一阶梯度。
    pub(crate) fn from_rgba(raw: &[u8], width: usize, height: usize) -> Self {
        let count = width.saturating_mul(height);
        let mut pixels = Vec::with_capacity(count);
        for chunk in raw.as_chunks::<4>().0.iter().take(count) {
            pixels.push(rgba_to_feature(chunk[0], chunk[1], chunk[2]));
        }
        let grad = compute_axis_gradients(&pixels, width, height);
        Self {
            pixels,
            grad,
            width,
            height,
        }
    }

    /// 将横向滚动画面的 RGBA 像素转置为纵向特征平面（原 X 轴映射为 Y 轴）。
    ///
    /// 采用 `16×16` 分块（Tile）在 L1 缓存栈缓冲区内完成转置，使源图像读取与目标缓冲区写入
    /// 均保持行连续局部性，避免大图转置时的缓存行抖动。
    pub(crate) fn from_rgba_transposed(raw: &[u8], width: usize, height: usize) -> Self {
        const TILE: usize = 16;
        let tw = height;
        let th = width;
        let mut pixels = vec![[0u8; 3]; tw.saturating_mul(th)];
        let expected_bytes = width.saturating_mul(height).saturating_mul(4);

        if raw.len() >= expected_bytes {
            let mut tile = [[[0u8; 3]; TILE]; TILE];
            for by in (0..height).step_by(TILE) {
                let block_h = (height - by).min(TILE);
                for bx in (0..width).step_by(TILE) {
                    let block_w = (width - bx).min(TILE);
                    for (ly, y) in (by..by + block_h).enumerate() {
                        let row_start = (y * width + bx) * 4;
                        let row_slice = &raw[row_start..row_start + block_w * 4];
                        for (lx, px) in row_slice.as_chunks::<4>().0.iter().enumerate() {
                            tile[lx][ly] = rgba_to_feature(px[0], px[1], px[2]);
                        }
                    }
                    for (lx, x) in (bx..bx + block_w).enumerate() {
                        let dst_start = x * tw + by;
                        pixels[dst_start..dst_start + block_h]
                            .copy_from_slice(&tile[lx][..block_h]);
                    }
                }
            }
        } else {
            for y in 0..height {
                for x in 0..width {
                    let src = (y.saturating_mul(width).saturating_add(x)).saturating_mul(4);
                    let dst = x.saturating_mul(tw).saturating_add(y);
                    if let (Some(px), Some(slot)) = (raw.get(src..src + 4), pixels.get_mut(dst)) {
                        *slot = rgba_to_feature(px[0], px[1], px[2]);
                    }
                }
            }
        }
        let grad = compute_axis_gradients(&pixels, tw, th);
        Self {
            pixels,
            grad,
            width: tw,
            height: th,
        }
    }

    /// 供固定双栏检测使用的纯灰度亮度拷贝。
    pub(crate) fn luma_plane(&self) -> Vec<u8> {
        self.pixels.iter().map(|p| p[0]).collect()
    }
}

#[inline]
pub(crate) fn rgba_to_feature(r: u8, g: u8, b: u8) -> FeatureRgb {
    let luma = ((u32::from(r) * 77 + u32::from(g) * 150 + u32::from(b) * 29) >> 8) as u8;
    let rg = (((i16::from(r) - i16::from(g)) >> 1) + 128).clamp(0, 255) as u8;
    let gb = (((i16::from(g) - i16::from(b)) >> 1) + 128).clamp(0, 255) as u8;
    [luma, rg, gb]
}

#[inline]
pub(crate) fn feature_max_diff(a: FeatureRgb, b: FeatureRgb) -> u8 {
    a[0].abs_diff(b[0])
        .max(a[1].abs_diff(b[1]))
        .max(a[2].abs_diff(b[2]))
}

fn compute_axis_gradients(pixels: &[FeatureRgb], width: usize, height: usize) -> Vec<u8> {
    let mut grad = vec![0u8; width.saturating_mul(height)];
    for y in 1..height {
        let row = y * width;
        let prev = (y - 1) * width;
        for x in 0..width {
            grad[row + x] = feature_max_diff(pixels[row + x], pixels[prev + x]);
        }
    }
    grad
}

/// 从活跃正文区域构造 `4×2` 高斯核平滑金字塔层（水平方向无相位差取 `4px` 盒均，垂直方向先做 `[1,2,1]` 平滑再降采 `2×`）。
///
/// 之所以采用 `4×2` 非对称降采样而非 `4×4` 硬分箱，是因为垂直滚动下水平方向完全同相无混叠，
/// 而垂直方向仅隔行降采 `2×` 叠加三抽头平滑可将最坏单像素台阶边缘相位差压缩至 `<= 1px`，彻底消除尖锐细线丢失。
pub(crate) fn downsample_roi_4x2(
    plane: &FeaturePlane,
    x_start: usize,
    x_end: usize,
    y_start: usize,
    active_height: usize,
) -> Option<FeaturePlane> {
    let cw = x_end.saturating_sub(x_start) / 4;
    let ch = active_height / 2;
    if cw < 4 || ch < 32 {
        return None;
    }
    let mut pixels = Vec::with_capacity(cw * ch);
    let mut grad = Vec::with_capacity(cw * ch);
    let y_limit = y_start + active_height;

    for cy in 0..ch {
        let fy = y_start + cy * 2;
        for cx in 0..cw {
            let fx0 = x_start + cx * 4;
            let (feat, g_max) = pool_4x3_tent_block(plane, fx0, fy, y_start, y_limit);
            pixels.push(feat);
            grad.push(g_max);
        }
    }
    Some(FeaturePlane {
        pixels,
        grad,
        width: cw,
        height: ch,
    })
}

fn pool_4x3_tent_block(
    plane: &FeaturePlane,
    fx0: usize,
    fy: usize,
    y_min: usize,
    y_max: usize,
) -> (FeatureRgb, u8) {
    let y_prev = fy.saturating_sub(1).max(y_min);
    let y_next = (fy + 1).min(y_max.saturating_sub(1));
    let mut s0 = 0u32;
    let mut s1 = 0u32;
    let mut s2 = 0u32;
    let mut g_max = 0u8;
    for (row_y, wy) in [(y_prev, 1u32), (fy, 2u32), (y_next, 1u32)] {
        let row = row_y * plane.width + fx0;
        for dx in 0..4 {
            let p = plane.pixels[row + dx];
            s0 += u32::from(p[0]) * wy;
            s1 += u32::from(p[1]) * wy;
            s2 += u32::from(p[2]) * wy;
            g_max = g_max.max(plane.grad[row + dx]);
        }
    }
    ([(s0 >> 4) as u8, (s1 >> 4) as u8, (s2 >> 4) as u8], g_max)
}

/// 在 `4×2` 粗尺度平面按局部波谷抽出前后向最值得精修的候选中点偏移（精细像素单位）。
pub(crate) fn pick_coarse_candidate_centers(
    prev_c: &FeaturePlane,
    curr_c: &FeaturePlane,
    max_fine_shift: usize,
) -> Vec<i32> {
    let max_cs = (max_fine_shift / 2).min(curr_c.height.saturating_sub(8));
    if max_cs < 2 {
        return Vec::new();
    }
    let mut centers = Vec::with_capacity(16);
    for sign in [1i32, -1i32] {
        let mut curve = Vec::with_capacity(max_cs);
        for cs in 2..=max_cs {
            let err = coarse_weighted_error(prev_c, curr_c, (cs as i32) * sign);
            curve.push((cs as i32 * sign * 2, err));
        }
        collect_top_valleys(&curve, 6, &mut centers);
    }
    centers
}

fn coarse_weighted_error(prev: &FeaturePlane, curr: &FeaturePlane, shift: i32) -> f64 {
    let abs_s = shift.unsigned_abs() as usize;
    let overlap = curr.height.saturating_sub(abs_s);
    if overlap < 4 {
        return f64::MAX;
    }
    let mut w_sad = 0u64;
    let mut w_sum = 0u64;
    for r in 0..overlap {
        let (py, cy) = if shift >= 0 {
            (r + abs_s, r)
        } else {
            (r, r + abs_s)
        };
        let p_row = py * prev.width;
        let c_row = cy * curr.width;
        for x in 0..curr.width {
            let w = prev.grad[p_row + x].max(curr.grad[c_row + x]);
            if w >= 4 {
                let d = u64::from(feature_max_diff(
                    prev.pixels[p_row + x],
                    curr.pixels[c_row + x],
                ));
                let weight = u64::from(w);
                w_sad += weight * d;
                w_sum += weight;
            }
        }
    }
    if w_sum == 0 {
        f64::MAX
    } else {
        (w_sad as f64) / (w_sum as f64)
    }
}

fn collect_top_valleys(curve: &[(i32, f64)], max_keep: usize, out: &mut Vec<i32>) {
    let mut valleys: Vec<(i32, f64)> = Vec::new();
    for i in 0..curve.len() {
        let err = curve[i].1;
        if !err.is_finite() || err == f64::MAX {
            continue;
        }
        let left_ok = i == 0 || err <= curve[i - 1].1;
        let right_ok = i + 1 == curve.len() || err <= curve[i + 1].1;
        if left_ok && right_ok {
            valleys.push(curve[i]);
        }
    }
    valleys.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (center, _) in valleys.into_iter().take(max_keep) {
        if !out.contains(&center) {
            out.push(center);
        }
    }
}

/// 重叠区多频带参数。
pub(crate) struct ConsensusRoi {
    pub(crate) x_start: usize,
    pub(crate) x_end: usize,
    pub(crate) y_start: usize,
    pub(crate) active_height: usize,
    pub(crate) shift: i32,
}

/// 在多个纵向子频带上计算零均值归一化互相关（ZNCC）达成结构共识。
///
/// 即便整帧出现全局轻微明暗偏移，同一文本结构在 ZNCC 下依然接近 1.0；
/// 而当仅上三分之一偶然对齐、其余三分之二为完全不同内容时，中位频带 ZNCC 会急剧跌破阈值并被拦截。
pub(crate) fn verify_multi_band_zncc(
    prev: &FeaturePlane,
    curr: &FeaturePlane,
    roi: &ConsensusRoi,
) -> bool {
    let abs_shift = roi.shift.unsigned_abs() as usize;
    let overlap = roi.active_height.saturating_sub(abs_shift);
    if overlap < 15 || roi.x_end <= roi.x_start {
        return false;
    }

    let num_bands = if overlap >= 45 { 3 } else { 1 };
    let mut band_scores = Vec::with_capacity(num_bands);
    for b in 0..num_bands {
        let r0 = (overlap * b) / num_bands;
        let r1 = (overlap * (b + 1)) / num_bands;
        if let Some(score) = compute_band_zncc(prev, curr, roi, r0, r1) {
            band_scores.push(score);
        }
    }
    if band_scores.is_empty() {
        return true;
    }
    band_scores.sort_by(f64::total_cmp);
    let median = band_scores[band_scores.len() / 2];
    median >= 0.62
}

fn compute_band_zncc(
    prev: &FeaturePlane,
    curr: &FeaturePlane,
    roi: &ConsensusRoi,
    r0: usize,
    r1: usize,
) -> Option<f64> {
    let abs_shift = roi.shift.unsigned_abs() as usize;
    let step_y = if r1 - r0 > 60 { 2 } else { 1 };
    let step_x = if roi.x_end - roi.x_start > 160 { 2 } else { 1 };
    let mut n = 0.0f64;
    let (mut sp, mut sq, mut sp2, mut sq2, mut spq) = (0.0, 0.0, 0.0, 0.0, 0.0);

    for r in (r0..r1).step_by(step_y) {
        let (py, cy) = if roi.shift >= 0 {
            (roi.y_start + r + abs_shift, roi.y_start + r)
        } else {
            (roi.y_start + r, roi.y_start + r + abs_shift)
        };
        let p_row = py * prev.width;
        let c_row = cy * curr.width;
        for x in (roi.x_start..roi.x_end).step_by(step_x) {
            let p = f64::from(prev.pixels[p_row + x][0]);
            let q = f64::from(curr.pixels[c_row + x][0]);
            n += 1.0;
            sp += p;
            sq += q;
            sp2 += p * p;
            sq2 += q * q;
            spq += p * q;
        }
    }
    if n < 16.0 {
        return None;
    }
    let var_p = (sp2 - (sp * sp) / n).max(0.0);
    let var_q = (sq2 - (sq * sq) / n).max(0.0);
    // 两侧皆为空白平坦频带时不参与投票，单侧有高方差结构而对侧空白则记 0.0 判负。
    if var_p / n < 9.0 && var_q / n < 9.0 {
        return None;
    }
    if var_p / n < 2.0 || var_q / n < 2.0 {
        return Some(0.0);
    }
    let cov = spq - (sp * sq) / n;
    Some((cov / (var_p * var_q).sqrt()).clamp(-1.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tiled_transposed_feature_plane_matches_exact_coordinates() {
        let width = 37usize;
        let height = 29usize;
        let mut raw = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                raw[idx] = ((x * 7 + y * 13) % 256) as u8;
                raw[idx + 1] = ((x * 11 + y * 5) % 256) as u8;
                raw[idx + 2] = ((x * 3 + y * 19) % 256) as u8;
                raw[idx + 3] = 255;
            }
        }

        let transposed = FeaturePlane::from_rgba_transposed(&raw, width, height);
        assert_eq!(transposed.width, height);
        assert_eq!(transposed.height, width);
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                let expected = rgba_to_feature(raw[idx], raw[idx + 1], raw[idx + 2]);
                assert_eq!(transposed.pixels[x * height + y], expected);
            }
        }
    }
}
