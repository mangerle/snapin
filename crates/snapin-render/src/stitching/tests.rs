//! 长截图拼接算法与状态机单元测试套件。

use super::matcher::estimate_shift;
use super::stitcher::LongCaptureStitcher;
use super::types::{StitchDirection, StitchOptions};
use image::{Rgba, RgbaImage};

/// 辅助生成带丰富几何条纹特征的合成图像，用于位移与拼接精度验证
fn create_test_pattern(width: u32, height: u32, offset_y: u32, sticky_header: u32) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    for y in 0..height {
        // 如果处于固定顶栏区域，则像素不随 offset_y 变化
        let effective_y = if y < sticky_header { y } else { y + offset_y };

        for x in 0..width {
            // 生成高频条纹与棋盘格，保证丰富的纹理特征
            let r = ((effective_y * 13 + x * 7) % 256) as u8;
            let g = ((effective_y * 23 + x * 11) % 256) as u8;
            let b = ((effective_y * 37 + x * 17) % 256) as u8;
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    img
}

/// 辅助生成沿水平方向平移的合成图像，覆盖横向长截图的固定侧栏与画布扩展逻辑。
fn create_horizontal_test_pattern(
    width: u32,
    height: u32,
    offset_x: u32,
    sticky_sidebar: u32,
) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let effective_x = if x < sticky_sidebar { x } else { x + offset_x };
            let mut seed = (u64::from(effective_x) << 32) | u64::from(y);
            seed ^= seed >> 30;
            seed = seed.wrapping_mul(0xbf58_476d_1ce4_e5b9);
            seed ^= seed >> 27;
            seed = seed.wrapping_mul(0x94d0_49bb_1331_11eb);
            seed ^= seed >> 31;
            let r = seed as u8;
            let g = (seed >> 8) as u8;
            let b = (seed >> 16) as u8;
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    img
}

#[test]
fn test_vertical_shift_detection() {
    let width = 200;
    let height = 200;
    let frame1 = create_test_pattern(width, height, 0, 0);
    // 向下滚动 25 像素：内容向上移动 25 像素
    let frame2 = create_test_pattern(width, height, 25, 0);

    let options = StitchOptions::default();
    let result = estimate_shift(&frame1, &frame2, StitchDirection::Vertical, &options);

    assert_eq!(result.offset, 25, "应精确检测出 25 像素的正向位移");
    assert!(result.confidence > 0.85, "匹配置信度应高于 0.85");
    assert_eq!(result.sticky_header_size, 0, "无固定顶栏");
}

#[test]
fn test_sticky_header_detection() {
    let width = 200;
    let height = 200;
    let sticky_h = 30;
    let frame1 = create_test_pattern(width, height, 0, sticky_h);
    let frame2 = create_test_pattern(width, height, 20, sticky_h);

    let options = StitchOptions::default();
    let result = estimate_shift(&frame1, &frame2, StitchDirection::Vertical, &options);

    assert_eq!(result.offset, 20, "带固定顶栏时应准确检测位移");
    assert!(
        result.sticky_header_size >= 28 && result.sticky_header_size <= 32,
        "应识别出约 30 像素的固定顶栏，实际: {}",
        result.sticky_header_size
    );
}

#[test]
fn test_horizontal_shift_detection_and_sticky_sidebar() {
    let width = 200;
    let height = 160;
    let sticky_w = 24;
    let frame1 = create_horizontal_test_pattern(width, height, 0, sticky_w);
    let frame2 = create_horizontal_test_pattern(width, height, 22, sticky_w);

    let result = estimate_shift(
        &frame1,
        &frame2,
        StitchDirection::Horizontal,
        &StitchOptions::default(),
    );

    assert_eq!(result.offset, 22, "应准确检测横向正向位移");
    assert!(result.confidence > 0.85, "横向匹配置信度应高于 0.85");
    assert!(
        result.sticky_header_size >= 22 && result.sticky_header_size <= 26,
        "应识别出约 24 像素的固定左侧栏，实际: {}",
        result.sticky_header_size
    );
}

#[test]
fn test_invalid_margin_ratio_is_defensively_clamped() {
    let frame1 = create_test_pattern(80, 60, 0, 0);
    let frame2 = create_test_pattern(80, 60, 8, 0);
    let options = StitchOptions {
        margin_ratio: 4.0,
        ..StitchOptions::default()
    };

    let result = estimate_shift(&frame1, &frame2, StitchDirection::Vertical, &options);

    assert!(result.confidence.is_finite());
}

#[test]
fn test_stitcher_canvas_expansion_and_rollback() {
    let width = 200;
    let height = 200;
    let frame0 = create_test_pattern(width, height, 0, 0);
    let mut stitcher =
        LongCaptureStitcher::new(frame0, StitchDirection::Vertical, StitchOptions::default());

    assert_eq!(stitcher.dimensions(), (200, 200));

    // 投喂向下滚动 20 像素的帧
    let frame1 = create_test_pattern(width, height, 20, 0);
    let res1 = stitcher.feed_frame(&frame1);
    assert_eq!(res1.offset, 20);
    assert_eq!(stitcher.dimensions(), (200, 220), "画布应扩展 20 像素高度");

    // 再次投喂向下滚动 15 像素（累计 35 像素）的帧
    let frame2 = create_test_pattern(width, height, 35, 0);
    let res2 = stitcher.feed_frame(&frame2);
    assert_eq!(res2.offset, 15);
    assert_eq!(stitcher.dimensions(), (200, 235), "画布应扩展至 235 像素");

    // 测试反向回滚（用户往上滑鼠标 10 像素，从 35 回到 25）
    let frame3 = create_test_pattern(width, height, 25, 0);
    let res3 = stitcher.feed_frame(&frame3);
    assert_eq!(res3.offset, -10, "应检测出反向回滚 10 像素");
    assert_eq!(
        stitcher.dimensions(),
        (200, 225),
        "反向回滚后画布应自动裁剪回退 10 像素"
    );
}

#[test]
fn test_horizontal_stitcher_canvas_expansion_crop_and_viewport() {
    let width = 180;
    let height = 120;
    let frame0 = create_horizontal_test_pattern(width, height, 0, 0);
    let mut stitcher = LongCaptureStitcher::new(
        frame0,
        StitchDirection::Horizontal,
        StitchOptions::default(),
    );

    let frame1 = create_horizontal_test_pattern(width, height, 24, 0);
    let first = stitcher.feed_frame(&frame1);
    assert_eq!(first.offset, 24);
    assert_eq!(stitcher.dimensions(), (204, 120));

    let frame2 = create_horizontal_test_pattern(width, height, 36, 0);
    let second = stitcher.feed_frame(&frame2);
    assert_eq!(second.offset, 12);
    assert_eq!(stitcher.dimensions(), (216, 120));

    let viewport = stitcher.current_viewport_window();
    assert_eq!(viewport.total, 216.0);
    assert_eq!(viewport.size, 180.0);
    assert_eq!(viewport.start, 36.0);

    stitcher.crop_tail(10);
    assert_eq!(stitcher.dimensions(), (206, 120));
    stitcher.crop_head(6);
    assert_eq!(stitcher.dimensions(), (200, 120));

    let rollback = create_horizontal_test_pattern(width, height, 24, 0);
    let result = stitcher.feed_frame(&rollback);
    assert_eq!(result.offset, -12);
    assert_eq!(stitcher.dimensions(), (188, 120));
}

#[test]
fn test_restart_with_direction_uses_latest_frame() {
    let frame = create_test_pattern(120, 160, 0, 0);
    let mut stitcher =
        LongCaptureStitcher::new(frame, StitchDirection::Vertical, StitchOptions::default());

    stitcher.restart_with_direction(StitchDirection::Horizontal);

    assert_eq!(stitcher.direction(), StitchDirection::Horizontal);
    assert_eq!(stitcher.dimensions(), (120, 160));
    assert_eq!(stitcher.initial_frame_size(), (120, 160));
}

#[test]
fn test_manual_crop_head_and_tail() {
    let width = 100;
    let height = 150;
    let frame0 = create_test_pattern(width, height, 0, 0);
    let mut stitcher =
        LongCaptureStitcher::new(frame0, StitchDirection::Vertical, StitchOptions::default());

    // 先扩展到 200 高度
    let frame1 = create_test_pattern(width, height, 50, 0);
    stitcher.feed_frame(&frame1);
    assert_eq!(stitcher.dimensions(), (100, 200));

    // 测试尾部裁剪 20 像素
    stitcher.crop_tail(20);
    assert_eq!(stitcher.dimensions(), (100, 180));

    // 测试头部裁剪 10 像素
    stitcher.crop_head(10);
    assert_eq!(stitcher.dimensions(), (100, 170));
}

#[test]
fn test_viewport_window_calculation() {
    let width = 100;
    let height = 100;
    let frame0 = create_test_pattern(width, height, 0, 0);
    let mut stitcher =
        LongCaptureStitcher::new(frame0, StitchDirection::Vertical, StitchOptions::default());

    let frame1 = create_test_pattern(width, height, 50, 0);
    stitcher.feed_frame(&frame1);
    // 当前画布高 150，帧高 100
    let vp = stitcher.current_viewport_window();
    assert_eq!(vp.total, 150.0);
    assert_eq!(vp.size, 100.0);
    assert_eq!(vp.start, 50.0);
    assert!((vp.normalized_start() - 50.0 / 150.0).abs() < 1e-4);
    assert!((vp.normalized_size() - 100.0 / 150.0).abs() < 1e-4);
}
