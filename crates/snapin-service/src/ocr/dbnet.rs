//! DBNet (Differentiable Binarization) 文本检测后处理。
//!
//! 对模型输出的单通道概率图执行阈值二值化、水平膨胀、连通域分析、置信度过滤与
//! unclip 外扩，最终输出原图坐标系下的水平文本框。水平膨胀用于把同一行内的字符
//! 合并为一个文本框，避免逐字切分成大量碎片。

use std::cmp::Ordering;
use std::collections::VecDeque;

/// DBNet 后处理参数（与模型 `ppocrv6_small_det.yml` 中的配置保持一致）
#[derive(Debug, Clone, Copy)]
pub struct DbPostProcessParams {
    /// 概率图二值化阈值
    pub thresh: f32,
    /// 文本框平均置信度阈值
    pub box_thresh: f32,
    /// 文本框外扩比例
    pub unclip_ratio: f32,
    /// 最大候选框数量
    pub max_candidates: usize,
}

impl Default for DbPostProcessParams {
    fn default() -> Self {
        Self {
            thresh: 0.3,
            box_thresh: 0.45,
            unclip_ratio: 1.4,
            max_candidates: 256,
        }
    }
}

/// 检测到的文本框
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedBox {
    /// 原图坐标系下的轴对齐包围盒 [x, y, width, height]
    pub bbox: (f32, f32, f32, f32),
    /// 文本框四边形角点（顺时针）
    pub polygon: [(f32, f32); 4],
    /// 概率图平均置信度
    pub score: f32,
}

/// 从 DBNet 输出的概率特征图中提取文本行边界框与多边形。
pub fn extract_bounding_boxes(
    prob_map: &[f32],
    map_w: usize,
    map_h: usize,
    orig_w: u32,
    orig_h: u32,
    params: &DbPostProcessParams,
) -> Vec<DetectedBox> {
    let Some(expected_len) = map_w.checked_mul(map_h) else {
        return Vec::new();
    };
    if map_w == 0 || map_h == 0 || orig_w == 0 || orig_h == 0 || prob_map.len() < expected_len {
        return Vec::new();
    }

    let mut mask = vec![false; expected_len];
    for (index, value) in prob_map.iter().take(expected_len).enumerate() {
        mask[index] = *value >= params.thresh;
    }

    let dilated = horizontal_dilate(&mask, map_w, map_h);

    let mut visited = vec![false; expected_len];
    let mut boxes: Vec<DetectedBox> = Vec::new();
    let scale_x = orig_w as f32 / map_w as f32;
    let scale_y = orig_h as f32 / map_h as f32;

    for y in 0..map_h {
        for x in 0..map_w {
            let index = y * map_w + x;
            if visited[index] || !dilated[index] {
                continue;
            }

            let mut queue = VecDeque::new();
            queue.push_back((x, y));
            visited[index] = true;

            let mut min_x = usize::MAX;
            let mut max_x = 0usize;
            let mut min_y = usize::MAX;
            let mut max_y = 0usize;
            let mut score_sum = 0.0f32;
            let mut area = 0usize;

            while let Some((cx, cy)) = queue.pop_front() {
                // 连通性由膨胀后的掩码决定，但文本框几何与置信度只统计原始概率图有效像素，
                // 避免膨胀操作把文本框向左右额外撑宽。
                if prob_map[cy * map_w + cx] >= params.thresh {
                    area += 1;
                    score_sum += prob_map[cy * map_w + cx];
                    min_x = min_x.min(cx);
                    max_x = max_x.max(cx);
                    min_y = min_y.min(cy);
                    max_y = max_y.max(cy);
                }

                for (nx, ny) in [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ] {
                    if nx < map_w && ny < map_h {
                        let neighbor = ny * map_w + nx;
                        if !visited[neighbor] && dilated[neighbor] {
                            visited[neighbor] = true;
                            queue.push_back((nx, ny));
                        }
                    }
                }
            }

            if area < 4 || min_x == usize::MAX || min_y == usize::MAX {
                continue;
            }

            let box_w = max_x - min_x + 1;
            let box_h = max_y - min_y + 1;
            if box_w < 3 || box_h < 3 {
                continue;
            }

            let score = score_sum / area as f32;
            if score < params.box_thresh {
                continue;
            }

            let mut x0 = min_x as f32 * scale_x;
            let mut y0 = min_y as f32 * scale_y;
            let mut x1 = (max_x + 1) as f32 * scale_x;
            let mut y1 = (max_y + 1) as f32 * scale_y;

            // 近似 PaddleOCR 的 unclip 外扩，避免紧贴文字边缘裁切掉笔画。
            //
            // 概率图是训练时按字高比例收缩过的文本核，外扩距离必须随文本尺寸线性增长：
            // 长条文本的面积/周长约为字高的一半，乘以 unclip_ratio 后即为每侧需要恢复的
            // 缩放量。这里严禁给距离设置固定像素上限，否则大字号文本只有字心被扩回，
            // 上下缘笔画仍留在框外，原位翻译的擦除矩形便盖不干净原文。
            let width = x1 - x0;
            let height = y1 - y0;
            let distance = width * height * params.unclip_ratio / (2.0 * (width + height));
            x0 = (x0 - distance).max(0.0);
            y0 = (y0 - distance).max(0.0);
            x1 = (x1 + distance).min(orig_w as f32);
            y1 = (y1 + distance).min(orig_h as f32);

            if x1 - x0 < 2.0 || y1 - y0 < 2.0 {
                continue;
            }

            boxes.push(DetectedBox {
                bbox: (x0, y0, x1 - x0, y1 - y0),
                polygon: [(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
                score,
            });
        }
    }

    sort_by_reading_order(&mut boxes);
    boxes.truncate(params.max_candidates);
    boxes
}

/// 按阅读顺序重排检测框：先按垂直重叠聚成行，行内自左向右。
///
/// 不能直接按 `(y, x)` 字典序排序：同一视觉行内左右两栏的检测框只要上边缘存在
/// 亚像素差异，整行就会左右颠倒。实测表格截图中左右两格的上边缘只差 0.11px，
/// 排序结果便是右列排在左列之前，复制出来的文本整行错序。
///
/// 这里改用垂直重叠判定同一行（重叠达到较矮框高度的一半），再在行内按 x 排序。
/// 相比「垂直中心距离小于固定容差」的做法，重叠判定不受 unclip 外扩干扰：外扩会
/// 把每行框的上下边界一起推出去，相邻行之间的间隙同步变大，不会因框高虚增而误并。
///
/// 已知局限：不解析多栏文档的列结构，只保证同一视觉行内从左到右；若相邻两行的
/// 框垂直重叠达到较矮框高度的一半（行距异常紧凑），会被并成同一行。
fn sort_by_reading_order(boxes: &mut [DetectedBox]) {
    if boxes.len() <= 1 {
        return;
    }

    let mut ordered: Vec<DetectedBox> = boxes.to_vec();
    ordered.sort_by(|left, right| {
        left.bbox
            .1
            .partial_cmp(&right.bbox.1)
            .unwrap_or(Ordering::Equal)
    });

    let mut rows: Vec<Vec<DetectedBox>> = Vec::new();
    for detected in ordered {
        let starts_new_row = match rows.last() {
            None => true,
            Some(row) => !same_visual_row(row[0].bbox, detected.bbox),
        };
        if starts_new_row {
            rows.push(Vec::with_capacity(4));
        }
        if let Some(row) = rows.last_mut() {
            row.push(detected);
        }
    }

    let mut index = 0usize;
    for row in &mut rows {
        row.sort_by(|left, right| {
            left.bbox
                .0
                .partial_cmp(&right.bbox.0)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    left.bbox
                        .1
                        .partial_cmp(&right.bbox.1)
                        .unwrap_or(Ordering::Equal)
                })
        });
        for detected in row.iter() {
            boxes[index] = *detected;
            index += 1;
        }
    }
}

/// 判断两个检测框是否属于同一视觉行：垂直重叠达到较矮框高度的一半。
fn same_visual_row(left: (f32, f32, f32, f32), right: (f32, f32, f32, f32)) -> bool {
    let left_bottom = left.1 + left.3;
    let right_bottom = right.1 + right.3;
    let overlap = left_bottom.min(right_bottom) - left.1.max(right.1);
    let smaller_height = left.3.min(right.3);
    if !smaller_height.is_finite() || smaller_height <= 0.0 {
        return false;
    }
    overlap > 0.0 && overlap >= smaller_height * 0.5
}

/// 对二值掩码执行水平膨胀，将同一文本行内相邻字符连接为同一连通域。
fn horizontal_dilate(mask: &[bool], width: usize, height: usize) -> Vec<bool> {
    let radius = ((height as f32 * 0.03) as usize).clamp(2, 20);
    let mut dilated = vec![false; mask.len()];

    for y in 0..height {
        let row_offset = y * width;
        for x in 0..width {
            if !mask[row_offset + x] {
                continue;
            }
            let end = (x + radius).min(width - 1);
            for target in x..=end {
                dilated[row_offset + target] = true;
            }
        }
    }

    dilated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_single_text_box() {
        let mut map = vec![0.0f32; 100 * 100];
        for y in 30..40 {
            for x in 20..40 {
                map[y * 100 + x] = 0.9;
            }
        }

        let boxes = extract_bounding_boxes(
            &map,
            100,
            100,
            200,
            200,
            &DbPostProcessParams {
                unclip_ratio: 0.0,
                ..DbPostProcessParams::default()
            },
        );

        assert_eq!(boxes.len(), 1);
        let bbox = boxes[0].bbox;
        assert!((bbox.0 - 40.0).abs() < 1.0);
        assert!((bbox.1 - 60.0).abs() < 1.0);
        assert!((bbox.2 - 40.0).abs() < 2.0);
        assert!((bbox.3 - 20.0).abs() < 2.0);
        assert!(boxes[0].score > 0.85);
    }

    /// 大字号文本的外扩距离必须随尺寸增长，不能被固定像素上限截断，
    /// 否则原位翻译擦除矩形只覆盖字心，笔画边缘会残留在原图上。
    #[test]
    fn test_large_text_box_unclip_scales_with_text_size() {
        let (map_w, map_h) = (300usize, 200usize);
        let mut map = vec![0.0f32; map_w * map_h];
        for y in 60..100 {
            for x in 50..190 {
                map[y * map_w + x] = 0.9;
            }
        }

        let boxes = extract_bounding_boxes(&map, map_w, map_h, 300, 200, &Default::default());
        assert_eq!(boxes.len(), 1);

        // 收缩核为 140x40，理论外扩距离约 21.8px，明显超过旧实现的 16px 上限
        let margin = 140.0 * 40.0 * DbPostProcessParams::default().unclip_ratio / (2.0 * 180.0);
        let (x, y, w, h) = boxes[0].bbox;
        assert!(
            (x - (50.0 - margin)).abs() < 1.0,
            "实际 x={x}，期望边距 {margin}"
        );
        assert!(
            (y - (60.0 - margin)).abs() < 1.0,
            "实际 y={y}，期望边距 {margin}"
        );
        assert!(
            (w - (140.0 + 2.0 * margin)).abs() < 2.0,
            "实际宽度 {w}，期望 {}",
            140.0 + 2.0 * margin
        );
        assert!(
            (h - (40.0 + 2.0 * margin)).abs() < 2.0,
            "实际高度 {h}，期望 {}",
            40.0 + 2.0 * margin
        );
    }

    #[test]
    fn test_extract_invalid_input_is_safe() {
        assert!(extract_bounding_boxes(&[], 0, 0, 100, 100, &Default::default()).is_empty());
        assert!(extract_bounding_boxes(&[0.9], 100, 100, 100, 100, &Default::default()).is_empty());
    }

    fn detected_box(bbox: (f32, f32, f32, f32)) -> DetectedBox {
        DetectedBox {
            bbox,
            polygon: [
                (bbox.0, bbox.1),
                (bbox.0 + bbox.2, bbox.1),
                (bbox.0 + bbox.2, bbox.1 + bbox.3),
                (bbox.0, bbox.1 + bbox.3),
            ],
            score: 0.9,
        }
    }

    /// 表格同一行内左右两格必须输出为「自左向右」，不能因为上边缘的亚像素差异
    /// （实测 0.11px）导致整行颠倒。用例数据取自真实截图识别结果。
    #[test]
    fn test_reading_order_groups_table_rows() {
        let mut boxes = vec![
            detected_box((343.09, 40.13, 57.81, 46.31)),
            detected_box((41.93, 40.24, 56.61, 46.08)),
            detected_box((43.36, 101.64, 55.68, 43.28)),
            detected_box((343.29, 103.20, 38.04, 43.91)),
            detected_box((342.25, 159.32, 58.52, 45.12)),
            detected_box((43.61, 160.95, 55.20, 41.85)),
        ];

        sort_by_reading_order(&mut boxes);

        let xs: Vec<f32> = boxes.iter().map(|box_| box_.bbox.0).collect();
        let expected = [41.93, 343.09, 43.36, 343.29, 43.61, 342.25];
        for (actual, wanted) in xs.iter().zip(expected.iter()) {
            assert!(
                (actual - wanted).abs() < 0.01,
                "阅读顺序不符合行优先，实际 x 序列: {xs:?}"
            );
        }
    }

    /// 行距正常的两行文本必须保持上下顺序，不得被并成一行。
    #[test]
    fn test_reading_order_keeps_lines_separate() {
        let mut boxes = vec![
            detected_box((10.0, 100.0, 80.0, 20.0)),
            detected_box((10.0, 10.0, 80.0, 20.0)),
        ];

        sort_by_reading_order(&mut boxes);

        assert!(boxes[0].bbox.1 < boxes[1].bbox.1);
    }

    #[test]
    fn test_reading_order_handles_empty_and_single() {
        let mut empty: Vec<DetectedBox> = Vec::new();
        sort_by_reading_order(&mut empty);
        assert!(empty.is_empty());

        let mut single = vec![detected_box((1.0, 2.0, 3.0, 4.0))];
        sort_by_reading_order(&mut single);
        assert_eq!(single.len(), 1);
    }

    #[test]
    fn test_low_score_box_is_filtered() {
        let mut map = vec![0.0f32; 64 * 64];
        for value in map.iter_mut().take(64 * 10).skip(64 * 2) {
            *value = 0.3;
        }
        let boxes = extract_bounding_boxes(&map, 64, 64, 64, 64, &Default::default());
        assert!(boxes.is_empty());
    }
}
