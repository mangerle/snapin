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
