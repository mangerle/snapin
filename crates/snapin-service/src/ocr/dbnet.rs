//! DBNet (Differentiable Binarization) 文本检测后处理。
//!
//! 对模型输出的单通道概率图执行阈值二值化、水平膨胀、连通域分析、置信度过滤与
//! unclip 外扩，最终输出原图坐标系下的水平文本框。水平膨胀用于把同一行内的字符
//! 合并为一个文本框，避免逐字切分成大量碎片。

use snapin_foundation::{Point, Rect};
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
            thresh: 0.2,
            box_thresh: 0.45,
            unclip_ratio: 1.4,
            max_candidates: 3000,
        }
    }
}

/// 检测到的文本框
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedBox {
    /// 原图坐标系下的轴对齐包围盒
    pub bbox: Rect,
    /// 文本框四边形角点（顺时针）
    pub polygon: [Point; 4],
    /// 概率图平均置信度
    pub score: f32,
}

/// 检测概率图与原始图像之间的尺寸契约。
///
/// 将四个尺寸集中为参数对象，避免调用方在连续整数参数中交换概率图尺寸和原图尺寸；
/// 所有字段均为正整数时构造成功，后处理内部无需重复校验零尺寸。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectionShape {
    /// 概率图宽度
    pub map_width: usize,
    /// 概率图高度
    pub map_height: usize,
    /// 原始图像宽度
    pub original_width: u32,
    /// 原始图像高度
    pub original_height: u32,
}

impl DetectionShape {
    /// 构造检测尺寸契约。
    pub const fn new(
        map_width: usize,
        map_height: usize,
        original_width: u32,
        original_height: u32,
    ) -> Self {
        Self {
            map_width,
            map_height,
            original_width,
            original_height,
        }
    }

    /// 判断所有尺寸是否可用于后处理。
    const fn is_valid(self) -> bool {
        self.map_width > 0
            && self.map_height > 0
            && self.original_width > 0
            && self.original_height > 0
    }
}

/// 从 DBNet 输出的概率特征图中提取文本行边界框与多边形。
pub fn extract_bounding_boxes(
    prob_map: &[f32],
    shape: DetectionShape,
    params: &DbPostProcessParams,
) -> Vec<DetectedBox> {
    let Some(mut detection) = DetectionMap::new(prob_map, shape, params) else {
        return Vec::new();
    };
    let mut boxes: Vec<DetectedBox> = Vec::with_capacity(params.max_candidates);

    for y in 0..shape.map_height {
        for x in 0..shape.map_width {
            if !detection.should_start_component(x, y) {
                continue;
            }
            let component = detection.collect_component(x, y);
            if let Some(detected) = detection.detected_box(component) {
                boxes.push(detected);
            }
        }
    }

    sort_by_reading_order(&mut boxes);
    boxes.truncate(params.max_candidates);
    boxes
}

/// 单次后处理的只读概率图与可变遍历状态。
struct DetectionMap<'a> {
    prob_map: &'a [f32],
    width: usize,
    height: usize,
    dilated: Vec<bool>,
    visited: Vec<bool>,
    scale_x: f32,
    scale_y: f32,
    original_width: u32,
    original_height: u32,
    params: &'a DbPostProcessParams,
}

impl<'a> DetectionMap<'a> {
    /// 校验尺寸并构建二值化、膨胀后的检测图。
    fn new(
        prob_map: &'a [f32],
        shape: DetectionShape,
        params: &'a DbPostProcessParams,
    ) -> Option<Self> {
        if !shape.is_valid() {
            return None;
        }
        let expected_len = shape.map_width.checked_mul(shape.map_height)?;
        if prob_map.len() < expected_len {
            return None;
        }

        let mask: Vec<bool> = prob_map
            .iter()
            .take(expected_len)
            .map(|value| *value >= params.thresh)
            .collect();
        let dilated = horizontal_dilate(&mask, shape.map_width, shape.map_height);
        Some(Self {
            prob_map,
            width: shape.map_width,
            height: shape.map_height,
            dilated,
            visited: vec![false; expected_len],
            scale_x: shape.original_width as f32 / shape.map_width as f32,
            scale_y: shape.original_height as f32 / shape.map_height as f32,
            original_width: shape.original_width,
            original_height: shape.original_height,
            params,
        })
    }

    /// 判断该像素是否是尚未访问的新连通域起点。
    fn should_start_component(&self, x: usize, y: usize) -> bool {
        let index = y * self.width + x;
        !self.visited[index] && self.dilated[index]
    }

    /// 从起点遍历一个连通域，仅统计原始概率图中超过阈值的像素几何。
    fn collect_component(&mut self, start_x: usize, start_y: usize) -> ComponentStats {
        let mut queue = VecDeque::with_capacity(64);
        queue.push_back((start_x, start_y));
        self.visited[start_y * self.width + start_x] = true;
        let mut stats = ComponentStats::default();

        while let Some((x, y)) = queue.pop_front() {
            let index = y * self.width + x;
            let value = self.prob_map[index];
            if value >= self.params.thresh {
                stats.add(x, y, value);
            }
            self.enqueue_neighbors(x, y, &mut queue);
        }
        stats
    }

    /// 访问四邻域中仍在膨胀掩码内且未入队的像素。
    fn enqueue_neighbors(&mut self, x: usize, y: usize, queue: &mut VecDeque<(usize, usize)>) {
        for (next_x, next_y) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if next_x >= self.width || next_y >= self.height {
                continue;
            }
            let neighbor = next_y * self.width + next_x;
            if !self.visited[neighbor] && self.dilated[neighbor] {
                self.visited[neighbor] = true;
                queue.push_back((next_x, next_y));
            }
        }
    }

    /// 过滤退化连通域并转换为原图坐标下的检测框。
    fn detected_box(&self, stats: ComponentStats) -> Option<DetectedBox> {
        if !stats.is_valid() {
            return None;
        }
        let score = stats.score_sum / stats.area as f32;
        if score < self.params.box_thresh {
            return None;
        }
        let bbox = self.unclipped_bounds(stats)?;
        Some(DetectedBox {
            bbox,
            polygon: [
                Point::new(bbox.x, bbox.y),
                Point::new(bbox.x + bbox.width, bbox.y),
                Point::new(bbox.x + bbox.width, bbox.y + bbox.height),
                Point::new(bbox.x, bbox.y + bbox.height),
            ],
            score,
        })
    }

    /// 近似 PaddleOCR 的 unclip 外扩，避免紧贴文字边缘裁切掉笔画。
    ///
    /// 概率图是训练时按字高比例收缩过的文本核，外扩距离必须随文本尺寸线性增长：
    /// 长条文本的面积/周长约为字高的一半，乘以 unclip_ratio 后即为每侧需要恢复的
    /// 缩放量。这里严禁给距离设置固定像素上限，否则大字号文本只有字心被扩回，
    /// 上下缘笔画仍留在框外，原位翻译的擦除矩形便盖不干净原文。
    fn unclipped_bounds(&self, stats: ComponentStats) -> Option<Rect> {
        let mut x0 = stats.min_x as f32 * self.scale_x;
        let mut y0 = stats.min_y as f32 * self.scale_y;
        let mut x1 = (stats.max_x + 1) as f32 * self.scale_x;
        let mut y1 = (stats.max_y + 1) as f32 * self.scale_y;
        let width = x1 - x0;
        let height = y1 - y0;
        let distance = width * height * self.params.unclip_ratio / (2.0 * (width + height));
        x0 = (x0 - distance).max(0.0);
        y0 = (y0 - distance).max(0.0);
        x1 = (x1 + distance).min(self.original_width as f32);
        y1 = (y1 + distance).min(self.original_height as f32);
        (x1 - x0 >= 2.0 && y1 - y0 >= 2.0).then_some(Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
}

/// 单个连通域的像素统计。
#[derive(Clone, Copy)]
struct ComponentStats {
    min_x: usize,
    max_x: usize,
    min_y: usize,
    max_y: usize,
    score_sum: f32,
    area: usize,
}

impl Default for ComponentStats {
    fn default() -> Self {
        Self {
            min_x: usize::MAX,
            max_x: 0,
            min_y: usize::MAX,
            max_y: 0,
            score_sum: 0.0,
            area: 0,
        }
    }
}

impl ComponentStats {
    /// 合并一个有效概率像素。
    fn add(&mut self, x: usize, y: usize, value: f32) {
        self.area += 1;
        self.score_sum += value;
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_y = self.min_y.min(y);
        self.max_y = self.max_y.max(y);
    }

    /// 判断连通域面积与包围尺寸是否达到最小文本框要求。
    fn is_valid(self) -> bool {
        self.area >= 4
            && self.min_x != usize::MAX
            && self.min_y != usize::MAX
            && self.max_x - self.min_x + 1 >= 3
            && self.max_y - self.min_y + 1 >= 3
    }
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
            .y
            .partial_cmp(&right.bbox.y)
            .unwrap_or(Ordering::Equal)
    });

    let mut rows: Vec<Vec<DetectedBox>> = Vec::with_capacity(boxes.len());
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
                .x
                .partial_cmp(&right.bbox.x)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    left.bbox
                        .y
                        .partial_cmp(&right.bbox.y)
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
fn same_visual_row(left: Rect, right: Rect) -> bool {
    let left_bottom = left.y + left.height;
    let right_bottom = right.y + right.height;
    let overlap = left_bottom.min(right_bottom) - left.y.max(right.y);
    let smaller_height = left.height.min(right.height);
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

    /// 后处理默认值必须与随模型发布的检测配置保持一致，避免静默漏检或截断候选框。
    #[test]
    fn test_default_params_match_model_det_config() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/models/ppocrv6/ppocrv6_small_det.yml");
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("读取检测模型配置失败，路径: {path:?}，原因: {error}"));
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(&content).expect("检测模型配置必须是合法 YAML");
        let post = &yaml["PostProcess"];
        let defaults = DbPostProcessParams::default();

        let thresh = post["thresh"].as_f64().expect("配置必须包含 thresh") as f32;
        let box_thresh = post["box_thresh"]
            .as_f64()
            .expect("配置必须包含 box_thresh") as f32;
        let unclip_ratio = post["unclip_ratio"]
            .as_f64()
            .expect("配置必须包含 unclip_ratio") as f32;
        let max_candidates = post["max_candidates"]
            .as_u64()
            .expect("配置必须包含 max_candidates") as usize;

        assert_eq!(defaults.thresh, thresh);
        assert_eq!(defaults.box_thresh, box_thresh);
        assert_eq!(defaults.unclip_ratio, unclip_ratio);
        assert_eq!(defaults.max_candidates, max_candidates);
    }

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
            DetectionShape::new(100, 100, 200, 200),
            &DbPostProcessParams {
                unclip_ratio: 0.0,
                ..DbPostProcessParams::default()
            },
        );

        assert_eq!(boxes.len(), 1);
        let bbox = boxes[0].bbox;
        assert!((bbox.x - 40.0).abs() < 1.0);
        assert!((bbox.y - 60.0).abs() < 1.0);
        assert!((bbox.width - 40.0).abs() < 2.0);
        assert!((bbox.height - 20.0).abs() < 2.0);
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

        let boxes = extract_bounding_boxes(
            &map,
            DetectionShape::new(map_w, map_h, 300, 200),
            &Default::default(),
        );
        assert_eq!(boxes.len(), 1);

        // 收缩核为 140x40，理论外扩距离约 21.8px，明显超过旧实现的 16px 上限
        let margin = 140.0 * 40.0 * DbPostProcessParams::default().unclip_ratio / (2.0 * 180.0);
        let Rect {
            x,
            y,
            width: w,
            height: h,
        } = boxes[0].bbox;
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
        assert!(extract_bounding_boxes(
            &[],
            DetectionShape::new(0, 0, 100, 100),
            &Default::default()
        )
        .is_empty());
        assert!(extract_bounding_boxes(
            &[0.9],
            DetectionShape::new(100, 100, 100, 100),
            &Default::default()
        )
        .is_empty());
    }

    fn detected_box(bbox: Rect) -> DetectedBox {
        DetectedBox {
            bbox,
            polygon: [
                Point::new(bbox.x, bbox.y),
                Point::new(bbox.x + bbox.width, bbox.y),
                Point::new(bbox.x + bbox.width, bbox.y + bbox.height),
                Point::new(bbox.x, bbox.y + bbox.height),
            ],
            score: 0.9,
        }
    }

    /// 表格同一行内左右两格必须输出为「自左向右」，不能因为上边缘的亚像素差异
    /// （实测 0.11px）导致整行颠倒。用例数据取自真实截图识别结果。
    #[test]
    fn test_reading_order_groups_table_rows() {
        let mut boxes = vec![
            detected_box(Rect::new(343.09, 40.13, 57.81, 46.31)),
            detected_box(Rect::new(41.93, 40.24, 56.61, 46.08)),
            detected_box(Rect::new(43.36, 101.64, 55.68, 43.28)),
            detected_box(Rect::new(343.29, 103.20, 38.04, 43.91)),
            detected_box(Rect::new(342.25, 159.32, 58.52, 45.12)),
            detected_box(Rect::new(43.61, 160.95, 55.20, 41.85)),
        ];

        sort_by_reading_order(&mut boxes);

        let xs: Vec<f32> = boxes.iter().map(|box_| box_.bbox.x).collect();
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
            detected_box(Rect::new(10.0, 100.0, 80.0, 20.0)),
            detected_box(Rect::new(10.0, 10.0, 80.0, 20.0)),
        ];

        sort_by_reading_order(&mut boxes);

        assert!(boxes[0].bbox.y < boxes[1].bbox.y);
    }

    #[test]
    fn test_reading_order_handles_empty_and_single() {
        let mut empty: Vec<DetectedBox> = Vec::new();
        sort_by_reading_order(&mut empty);
        assert!(empty.is_empty());

        let mut single = vec![detected_box(Rect::new(1.0, 2.0, 3.0, 4.0))];
        sort_by_reading_order(&mut single);
        assert_eq!(single.len(), 1);
    }

    #[test]
    fn test_low_score_box_is_filtered() {
        let mut map = vec![0.0f32; 64 * 64];
        for value in map.iter_mut().take(64 * 10).skip(64 * 2) {
            *value = 0.3;
        }
        let boxes = extract_bounding_boxes(
            &map,
            DetectionShape::new(64, 64, 64, 64),
            &Default::default(),
        );
        assert!(boxes.is_empty());
    }
}
