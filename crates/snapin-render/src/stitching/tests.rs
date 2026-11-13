//! 长截图拼接算法与状态机单元测试套件。

use super::matcher::estimate_shift;
use super::stitcher::LongCaptureStitcher;
use super::types::{
    FrameShiftAnalysis, MatchResult, StitchDirection, StitchFrameOutcome, StitchOptions,
};
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

    // crop_tail(10) 后画布末端对应 x=26..206，故馈入 x=24..204 的回滚帧真实位移应为 -2，
    // 裁掉最后 2px 后画布精确收敛为 (6..204) 即宽度 198，不再因 last_frame 滞后而双倍误删 10px。
    let rollback = create_horizontal_test_pattern(width, height, 24, 0);
    let result = stitcher.feed_frame(&rollback);
    assert_eq!(result.offset, -2);
    assert_eq!(stitcher.dimensions(), (198, 120));
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

#[test]
fn test_feed_frame_outcome_distinguishes_unchanged_unmatched_and_ignored() {
    let width = 120;
    let height = 160;
    let frame0 = create_test_pattern(width, height, 0, 0);
    let mut stitcher = LongCaptureStitcher::new(
        frame0.clone(),
        StitchDirection::Vertical,
        StitchOptions {
            auto_crop_reverse: false,
            ..StitchOptions::default()
        },
    );

    // 1. 完全静止帧返回 Unchanged，画布和参考帧不变
    assert_eq!(
        stitcher.feed_frame_outcome(&frame0),
        StitchFrameOutcome::Unchanged
    );
    assert_eq!(stitcher.dimensions(), (width, height));

    // 2. 仅 2x12 光标闪烁仍判定为 Unchanged，不进入失配流程
    let mut cursor_blink = frame0.clone();
    for y in 40..52 {
        for x in 60..62 {
            cursor_blink.put_pixel(x, y, Rgba([255, 255, 255, 255]));
        }
    }
    assert_eq!(
        stitcher.feed_frame_outcome(&cursor_blink),
        StitchFrameOutcome::Unchanged
    );

    // 3. 正常正向滚动 40px 推进画布并更新 last_frame
    let frame1 = create_test_pattern(width, height, 40, 0);
    assert!(matches!(
        stitcher.feed_frame_outcome(&frame1),
        StitchFrameOutcome::Stitched { offset: 40, .. }
    ));
    assert_eq!(stitcher.dimensions(), (width, 200));
    assert_eq!(stitcher.last_frame(), &frame1);

    // 4. 关闭反向自动裁剪时回退到 frame0：返回 Ignored，不得覆盖 last_frame
    assert_eq!(
        stitcher.feed_frame_outcome(&frame0),
        StitchFrameOutcome::Ignored
    );
    assert_eq!(stitcher.dimensions(), (width, 200));
    assert_eq!(stitcher.last_frame(), &frame1);

    // 5. 完全无关的高对比噪声帧（重叠失效）必须返回 Unmatched 且保留原基准帧 frame1
    let mut unmatched = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let v = if ((x / 7) + (y / 7)) % 2 == 0 { 0 } else { 255 };
            unmatched.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    assert!(matches!(
        stitcher.feed_frame_outcome(&unmatched),
        StitchFrameOutcome::Unmatched { .. }
    ));
    assert_eq!(stitcher.dimensions(), (width, 200));
    assert_eq!(stitcher.last_frame(), &frame1);

    // 6. 尺寸异常帧返回 Ignored
    let wrong_size = create_test_pattern(width + 10, height, 0, 0);
    assert_eq!(
        stitcher.feed_frame_outcome(&wrong_size),
        StitchFrameOutcome::Ignored
    );
}

#[test]
fn test_reversible_crop_past_initial_size_syncs_tail_last_frame_and_undoes() {
    let width = 100;
    let height = 120;
    let frame0 = create_test_pattern(width, height, 0, 0);
    let mut stitcher = LongCaptureStitcher::new(
        frame0.clone(),
        StitchDirection::Vertical,
        StitchOptions::default(),
    );

    // 先推进 +60 -> 总高 180
    let frame1 = create_test_pattern(width, height, 60, 0);
    assert!(stitcher.feed_frame_outcome(&frame1).is_stitched());
    assert_eq!(stitcher.dimensions(), (100, 180));

    // 手动尾部裁剪 30 -> 总高 150 (>= 120)：必须把 last_frame 同步到 [30..150]
    assert_eq!(stitcher.crop_tail(30), 30);
    assert_eq!(stitcher.dimensions(), (100, 150));
    assert!(!stitcher.is_tail_truncated_below_frame());
    assert_eq!(
        stitcher.last_frame(),
        &create_test_pattern(width, height, 30, 0),
        "crop_tail 后仍 >= 单帧高时，last_frame 必须同步为新末端切片"
    );

    // 静止帧 Unchanged 不清空手工裁剪历史
    assert_eq!(
        stitcher.feed_frame_outcome(&create_test_pattern(width, height, 30, 0)),
        StitchFrameOutcome::Unchanged
    );
    assert!(stitcher.can_undo_crop());

    // 在紧接新末端之后继续滚动 +25（即 y=55）应当完美无缝拼接 -> 175
    assert!(stitcher
        .feed_frame_outcome(&create_test_pattern(width, height, 55, 0))
        .is_stitched());
    assert_eq!(stitcher.dimensions(), (100, 175));
    // 正向推进拼入新像素后清空此前裁剪撤销历史
    assert!(!stitcher.can_undo_crop());

    // 允许突破 initial_frame_size(120) 进行头部与尾部裁剪，直至最小保底 MIN_CROPPED_SPAN(32)
    assert_eq!(stitcher.crop_head(40), 40);
    assert_eq!(stitcher.dimensions(), (100, 135));
    assert_eq!(stitcher.crop_tail(70), 70);
    assert_eq!(stitcher.dimensions(), (100, 65));
    assert!(stitcher.is_tail_truncated_below_frame());
    // 尾端已截断至单帧以内时保护禁止断层继续拼接
    assert_eq!(
        stitcher.feed_frame_outcome(&create_test_pattern(width, height, 80, 0)),
        StitchFrameOutcome::Ignored
    );

    // 继续大幅裁剪至最小保底 32px，再度裁剪返回 0
    assert_eq!(stitcher.crop_tail(100), 33);
    assert_eq!(stitcher.dimensions(), (100, 32));
    assert_eq!(stitcher.crop_head(20), 0);

    // 连续撤销三步手工裁剪，无损恢复至 (100, 175) 与可执行状态
    assert!(stitcher.undo_crop());
    assert_eq!(stitcher.dimensions(), (100, 65));
    assert!(stitcher.undo_crop());
    assert_eq!(stitcher.dimensions(), (100, 135));
    assert!(!stitcher.is_tail_truncated_below_frame());
    assert!(stitcher.undo_crop());
    assert_eq!(stitcher.dimensions(), (100, 175));
    assert!(!stitcher.can_undo_crop());
}

#[test]
fn test_extreme_aspect_thumbnail_overview_preserves_readable_cross_axis() {
    // 使用步长 80 (< active_height/2 = 100) 构造 240x6120 纵向极端长宽比画布（25.5:1）
    let frame0 = create_test_pattern(240, 200, 0, 0);
    let mut stitcher =
        LongCaptureStitcher::new(frame0, StitchDirection::Vertical, StitchOptions::default());
    for step in 1..=74u32 {
        let next = create_test_pattern(240, 200, step * 80, 0);
        assert!(stitcher.feed_frame_outcome(&next).is_stitched());
    }
    assert_eq!(stitcher.dimensions(), (240, 6120));

    // 若整图强缩进 124x244 会将宽挤到 10px，开启最小可读厚度 48px 后应切换尾部高清切片
    let overview = stitcher
        .get_or_render_thumbnail_overview(124, 244, 48)
        .clone();
    assert!(overview.is_cropped_window);
    assert!(overview.image.width() >= 48);
    assert_eq!(overview.slice_end_ratio, 1.0);
    assert!(overview.slice_start_ratio > 0.5 && overview.slice_start_ratio < 0.99);

    // 横向极端长宽比 6120x240 同理维持高度可读
    let h_first = create_horizontal_test_pattern(200, 240, 0, 0);
    let mut h_stitcher = LongCaptureStitcher::new(
        h_first,
        StitchDirection::Horizontal,
        StitchOptions::default(),
    );
    for step in 1..=74u32 {
        let next = create_horizontal_test_pattern(200, 240, step * 80, 0);
        assert!(h_stitcher.feed_frame_outcome(&next).is_stitched());
    }
    assert_eq!(h_stitcher.dimensions(), (6120, 240));
    let h_overview = h_stitcher
        .get_or_render_thumbnail_overview(244, 124, 48)
        .clone();
    assert!(h_overview.is_cropped_window);
    assert!(h_overview.image.height() >= 48);
    assert!(h_overview.slice_start_ratio > 0.5 && h_overview.slice_start_ratio < 0.99);
}

#[test]
fn test_sticky_header_and_footer_do_not_duplicate_and_rollback_cleanly() {
    // 构造顶部 20px 固定标题栏 + 底部 24px 固定工具栏 + 中间真实滚动正文的场景。
    let width = 120u32;
    let height = 220u32;
    let header = 20u32;
    let footer = 24u32;

    let build_frame = |scroll: u32| -> RgbaImage {
        let mut img = create_test_pattern(width, height, scroll, 0);
        for y in 0..header {
            for x in 0..width {
                let stripe = ((x / 8) % 2) as u8 * 40;
                img.put_pixel(x, y, Rgba([30 + stripe, 50 + (y as u8) * 3, 120, 255]));
            }
        }
        for y in (height - footer)..height {
            let rel_y = y - (height - footer);
            for x in 0..width {
                let tab = ((x / 15) % 2) as u8 * 50;
                img.put_pixel(x, y, Rgba([20 + tab, 40 + (rel_y as u8) * 4, 210, 255]));
            }
        }
        img
    };

    let f0 = build_frame(0);
    let f1 = build_frame(60);
    let f2 = build_frame(120);

    let mut stitcher =
        LongCaptureStitcher::new(f0, StitchDirection::Vertical, StitchOptions::default());

    let out1 = stitcher.feed_frame_outcome(&f1);
    match out1 {
        StitchFrameOutcome::Stitched {
            offset,
            sticky_header_size,
            sticky_footer_size,
            ..
        } => {
            assert!(
                (offset - 60).abs() <= 1,
                "期望正向位移 ~60px，实际 {offset}"
            );
            assert!(sticky_header_size >= 16, "应检测到固定顶栏");
            assert!(sticky_footer_size >= 16, "应检测到固定底栏");
        }
        other => panic!("期望 Stitched，实际 {other:?}"),
    }

    let out2 = stitcher.feed_frame_outcome(&f2);
    assert!(out2.is_stitched());
    assert_eq!(stitcher.dimensions(), (120, 340));

    // 验证拼接结果中原本被底栏遮挡的正文行没有丢失，且顶栏/底栏只保留各自一处。
    let stitched = stitcher.canvas();
    let probe_body_y = height - footer + 10;
    let body_ref = create_test_pattern(width, 400, 0, 0);
    let expected_body_pixel = *body_ref.get_pixel(35, probe_body_y);
    let actual_body_pixel = *stitched.get_pixel(35, probe_body_y);
    assert_eq!(
        actual_body_pixel, expected_body_pixel,
        "底栏剥离后中间连续正文像素应准确衔接，而非重复粘贴底栏"
    );

    // 验证底端唯一保留的固定底栏像素完整。
    let footer_ref = build_frame(120);
    let expected_footer_pixel = *footer_ref.get_pixel(10, height - 10);
    let actual_footer_pixel = *stitched.get_pixel(10, stitched.height() - 10);
    assert_eq!(actual_footer_pixel, expected_footer_pixel);

    // 向前回滚 60px 时，不仅回滚正文，也应恢复底栏至最末位置。
    let f1_back = build_frame(60);
    let back_out = stitcher.feed_frame_outcome(&f1_back);
    assert!(
        matches!(back_out, StitchFrameOutcome::Stitched { offset, .. } if offset < 0),
        "期望负位移回滚，实际 {back_out:?}"
    );
    assert_eq!(stitcher.dimensions(), (120, 280));
    assert_eq!(
        *stitcher
            .canvas()
            .get_pixel(10, stitcher.dimensions().1 - 10),
        expected_footer_pixel
    );
}

#[test]
fn test_horizontal_sticky_right_sidebar_avoidance() {
    // 构造左侧 20px 固定导航 + 右侧 24px 固定工具条的横向滚动场景。
    let width = 220u32;
    let height = 100u32;
    let left_bar = 20u32;
    let right_bar = 24u32;

    let build_frame = |scroll_x: u32| -> RgbaImage {
        let mut img = create_horizontal_test_pattern(width, height, scroll_x, 0);
        for x in 0..left_bar {
            for y in 0..height {
                let stripe = ((y / 8) % 2) as u8 * 45;
                img.put_pixel(x, y, Rgba([25 + (x as u8) * 3, 70 + stripe, 140, 255]));
            }
        }
        for x in (width - right_bar)..width {
            let rel_x = x - (width - right_bar);
            for y in 0..height {
                let tab = ((y / 12) % 2) as u8 * 65;
                img.put_pixel(
                    x,
                    y,
                    Rgba([200 - tab, 30 + (rel_x as u8) * 4, 60 + tab, 255]),
                );
            }
        }
        img
    };

    let mut stitcher = LongCaptureStitcher::new(
        build_frame(0),
        StitchDirection::Horizontal,
        StitchOptions::default(),
    );
    let out1 = stitcher.feed_frame_outcome(&build_frame(60));
    match out1 {
        StitchFrameOutcome::Stitched {
            offset,
            sticky_header_size,
            sticky_footer_size,
            ..
        } => {
            assert!(
                (offset - 60).abs() <= 1,
                "横向期望位移 ~60px，实际 {offset}"
            );
            assert!(sticky_header_size >= 16);
            assert!(sticky_footer_size >= 16);
        }
        other => panic!("期望 Stitched，实际 {other:?}"),
    }
    assert_eq!(stitcher.dimensions(), (280, 100));
    assert_eq!(
        stitcher
            .canvas()
            .get_pixel(stitcher.dimensions().0 - 10, 25),
        build_frame(60).get_pixel(width - 10, 25)
    );
}

#[test]
fn test_floating_overlay_robust_matching_and_clean_backfill_healing() {
    // 构造正文右下角存在 24x24px 悬浮按钮（不随滚动移动）的场景。
    let width = 160u32;
    let height = 220u32;
    let btn_x0 = 116u32;
    let btn_x1 = 140u32;
    let btn_y0 = 160u32;
    let btn_y1 = 184u32;

    let build_frame = |scroll: u32| -> RgbaImage {
        let mut img = create_test_pattern(width, height, scroll, 0);
        for y in btn_y0..btn_y1 {
            for x in btn_x0..btn_x1 {
                let ring = ((x - btn_x0) / 4 + (y - btn_y0) / 4) as u8 * 20;
                img.put_pixel(x, y, Rgba([240, 60 + ring % 120, 40, 255]));
            }
        }
        img
    };

    let f0 = build_frame(0);
    let f1 = build_frame(60);
    let mut stitcher =
        LongCaptureStitcher::new(f0, StitchDirection::Vertical, StitchOptions::default());

    let out1 = stitcher.feed_frame_outcome(&f1);
    match out1 {
        StitchFrameOutcome::Stitched { offset, .. } => {
            assert!(
                (offset - 60).abs() <= 1,
                "即使被局部悬浮按钮遮挡，也应精准匹配偏移 60px，实际 {offset}"
            );
        }
        other => panic!("期望 Stitched，实际 {other:?}"),
    }

    // 原本在第一帧 (btn_x0..btn_x1, 160..184) 处被悬浮按钮挡住的内容，
    // 在第二帧滚动 60px 后移到了帧内 y = 100..124（已脱出按钮），应被干净地自愈回填！
    let expected_clean = create_test_pattern(width, height, 0, 0);
    for y in btn_y0..btn_y1 {
        for x in btn_x0..btn_x1 {
            assert_eq!(
                stitcher.canvas().get_pixel(x, y),
                expected_clean.get_pixel(x, y),
                "位置 ({x}, {y}) 的悬浮按钮残影应被第二帧新露出的干净背景像素覆写治愈"
            );
        }
    }
}

#[test]
fn test_solid_and_vertical_stripe_regions_reject_min_step_drift() {
    let options = StitchOptions::default();

    // 1. 纯白背景仅局部角落轻微闪烁：不得漂移匹配为 min_step=2。
    let solid0 = RgbaImage::from_pixel(160, 200, Rgba([248, 248, 248, 255]));
    let mut solid1 = solid0.clone();
    for y in 185..195 {
        for x in 20..32 {
            solid1.put_pixel(x, y, Rgba([180, 180, 190, 255]));
        }
    }
    let res_solid = estimate_shift(&solid0, &solid1, StitchDirection::Vertical, &options);
    assert!(
        !res_solid.is_matched(options.confidence_threshold),
        "无沿轴结构纹理的纯色画面不得误判为 min_step 伪位移: {res_solid:?}"
    );
    let mut stitcher = LongCaptureStitcher::new(solid0, StitchDirection::Vertical, options);
    assert!(matches!(
        stitcher.feed_frame_outcome(&solid1),
        StitchFrameOutcome::Unmatched { .. }
    ));
    assert_eq!(stitcher.dimensions(), (160, 200));

    // 2. 纯垂直条纹（垂直于滚动方向梯度全为 0，平行方向永远不变）：同样不得发生 min_step 漂移。
    let mut vstripe0 = RgbaImage::new(160, 200);
    for y in 0..200 {
        for x in 0..160 {
            let v = if (x / 10) % 2 == 0 { 240 } else { 40 };
            vstripe0.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    let mut vstripe1 = vstripe0.clone();
    for x in 40..50 {
        vstripe1.put_pixel(x, 192, Rgba([20, 120, 220, 255]));
    }
    let res_vstripe = estimate_shift(&vstripe0, &vstripe1, StitchDirection::Vertical, &options);
    assert!(
        !res_vstripe.is_matched(options.confidence_threshold),
        "纯纵向条纹缺乏主轴纵向梯度，无法锁定滚动位移: {res_vstripe:?}"
    );
}

#[test]
fn test_periodic_repetitive_stripes_rejected_by_distinctiveness_guard() {
    let options = StitchOptions::default();
    let width = 160u32;
    let height = 220u32;
    let period = 24u32;

    let build_periodic = |phase: u32| -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            let py = (y + phase) % period;
            let luma = if py < 10 { 245 } else { 70 };
            for x in 0..width {
                let accent = if (x / 16) % 2 == 0 { 0 } else { 15 };
                img.put_pixel(x, y, Rgba([luma - accent, luma, luma - accent, 255]));
            }
        }
        img
    };

    let f0 = build_periodic(0);
    let f1 = build_periodic(12); // 半周期偏移：err(0) 很大，且 +12/+36/+60/-12 全等价
    let res = estimate_shift(&f0, &f1, StitchDirection::Vertical, &options);
    assert!(
        !res.is_matched(options.confidence_threshold),
        "等周期重复条纹在多个非邻域位移均可完美对齐，应由主峰独特性拦截而非盲目猜错步长: {res:?}"
    );
}

#[test]
fn test_sparse_text_over_periodic_zebra_rows_still_matches_uniquely() {
    let options = StitchOptions::default();
    let width = 200u32;
    let height = 220u32;
    let row_h = 24u32;

    let build_zebra_table = |scroll: u32| -> RgbaImage {
        let mut img = RgbaImage::from_pixel(width, height, Rgba([250, 250, 250, 255]));
        for y in 0..height {
            let doc_y = y + scroll;
            let row_idx = doc_y / row_h;
            let in_row = doc_y % row_h;
            let bg = if row_idx.is_multiple_of(2) { 248 } else { 234 };
            for x in 0..width {
                img.put_pixel(x, y, Rgba([bg, bg, bg, 255]));
            }
            // 在每一行中间绘制独一无二的拟真高对比短文本笔画块（x 位置与宽度随行号唯一变化）。
            if (6..16).contains(&in_row) {
                let tok_x0 = 32 + ((row_idx * 37) % 70);
                let tok_len = 24 + ((row_idx * 13) % 36);
                for x in tok_x0..(tok_x0 + tok_len).min(width - 25) {
                    if !(x + in_row).is_multiple_of(3) {
                        img.put_pixel(x, y, Rgba([30, 35, 45, 255]));
                    }
                }
            }
        }
        img
    };

    let f0 = build_zebra_table(0);
    let f1 = build_zebra_table(24);
    let res = estimate_shift(&f0, &f1, StitchDirection::Vertical, &options);
    assert!(
        res.is_matched(options.confidence_threshold),
        "带唯一文本特征的斑马纹表格应当精准唯一匹配: {res:?}"
    );
    assert_eq!(res.offset, 24);
}

#[test]
fn test_legitimate_minimal_two_pixel_scroll_still_accepted() {
    let options = StitchOptions::default();
    let f0 = create_test_pattern(160, 200, 0, 0);
    let f1 = create_test_pattern(160, 200, 2, 0);
    let res = estimate_shift(&f0, &f1, StitchDirection::Vertical, &options);
    assert!(res.is_matched(options.confidence_threshold));
    assert_eq!(res.offset, 2);
}

#[test]
fn test_rollback_noop_preserves_anchor_and_generation_and_clamped_rollback_syncs_anchor() {
    use super::types::{FrameShiftAnalysis, MatchResult};

    let width = 120;
    let height = 180;
    let f0 = create_test_pattern(width, height, 40, 0);
    let mut stitcher = LongCaptureStitcher::new(
        f0.clone(),
        StitchDirection::Vertical,
        StitchOptions::default(),
    );

    // 正向推进 +10px 后，构造 attached_footer_size 大于当前帧 footer 的临界场景，确保不会缩到单帧以下且零裁剪不污染锚点
    let f1 = create_test_pattern(width, height, 50, 0);
    assert!(stitcher.feed_frame_outcome(&f1).is_stitched());
    stitcher.attached_footer_size = 30;
    let gen_before = stitcher.generation();
    let anchor_before = stitcher.last_frame_arc();

    let overshot = create_test_pattern(width, height, 20, 0);
    let noop_outcome = stitcher.apply_analyzed_shift(
        overshot,
        gen_before,
        FrameShiftAnalysis::Matched(MatchResult {
            offset: -15,
            confidence: 0.95,
            sticky_header_size: 0,
            sticky_footer_size: 10,
        }),
    );
    assert_eq!(noop_outcome, StitchFrameOutcome::Ignored);
    assert_eq!(stitcher.generation(), gen_before);
    assert_eq!(stitcher.last_frame(), anchor_before.as_ref());
    assert_eq!(stitcher.dimensions(), (width, 190));

    // 当回滚位移超过可裁余量 (crop=10 < pixels=25) 时，锚点帧必须同步为实际画布末端切片而非越界帧
    stitcher.attached_footer_size = 0;
    let over_rollback = create_test_pattern(width, height, 25, 0);
    let clamped_outcome = stitcher.apply_analyzed_shift(
        over_rollback,
        gen_before,
        FrameShiftAnalysis::Matched(MatchResult {
            offset: -25,
            confidence: 0.95,
            sticky_header_size: 0,
            sticky_footer_size: 0,
        }),
    );
    assert!(clamped_outcome.is_stitched());
    assert_eq!(stitcher.dimensions(), (width, height));
    assert_eq!(stitcher.growth_polarity(), None);
    assert_eq!(stitcher.last_frame(), &f0);
}

#[test]
fn test_opposite_edge_crop_history_survives_stitch_and_restores_without_clobbering_anchor() {
    let width = 120;
    let height = 200;
    let f0 = create_test_pattern(width, height, 0, 0);
    let mut stitcher =
        LongCaptureStitcher::new(f0, StitchDirection::Vertical, StitchOptions::default());

    let f1 = create_test_pattern(width, height, 60, 0);
    assert!(stitcher.feed_frame_outcome(&f1).is_stitched());
    assert_eq!(stitcher.dimensions(), (width, 260));

    // 先裁剪头部，再继续正向向下拼接新帧
    assert_eq!(stitcher.crop_head(30), 30);
    assert_eq!(stitcher.dimensions(), (width, 230));
    assert!(stitcher.can_undo_crop());

    let f2 = create_test_pattern(width, height, 100, 0);
    assert!(stitcher.feed_frame_outcome(&f2).is_stitched());
    assert_eq!(stitcher.dimensions(), (width, 270));
    // 尾部增长不影响头部已裁剪区域的几何一致性，头部裁剪记录应保留
    assert!(stitcher.can_undo_crop());

    let anchor_before_undo = stitcher.last_frame_arc();
    assert!(stitcher.undo_crop());
    // 撤销后恢复头部 30px，新拼接的尾部与当前锚点帧保持不变
    assert_eq!(stitcher.dimensions(), (width, 300));
    assert_eq!(stitcher.last_frame(), anchor_before_undo.as_ref());
    assert!(!stitcher.can_undo_crop());
}

#[test]
fn test_extreme_aspect_thumbnail_anchors_to_cropped_head_then_returns_to_tail() {
    use super::types::{FrameShiftAnalysis, MatchResult};

    let width = 200;
    let frame_h = 400;
    let f0 = create_test_pattern(width, frame_h, 0, 0);
    let mut stitcher =
        LongCaptureStitcher::new(f0, StitchDirection::Vertical, StitchOptions::default());

    for step in 1..=4 {
        let g = stitcher.generation();
        let next = create_test_pattern(width, frame_h, step * 280, 0);
        assert!(stitcher
            .apply_analyzed_shift(
                next,
                g,
                FrameShiftAnalysis::Matched(MatchResult {
                    offset: 280,
                    confidence: 0.95,
                    sticky_header_size: 0,
                    sticky_footer_size: 0,
                }),
            )
            .is_stitched());
    }
    assert_eq!(stitcher.dimensions(), (200, 1520));

    // 默认正向增长停留在尾部切片；执行头部裁剪后缩略图必须切换锚定至头部，便于确认裁切位置
    let tail_overview = stitcher.get_or_render_thumbnail_overview(64, 240, 48);
    assert!(tail_overview.is_cropped_window);
    assert!((tail_overview.slice_end_ratio - 1.0).abs() < 1e-4);

    assert_eq!(stitcher.crop_head(40), 40);
    let head_overview = stitcher.get_or_render_thumbnail_overview(64, 240, 48);
    assert!(head_overview.is_cropped_window);
    assert!(head_overview.slice_start_ratio <= 1e-4);
    assert!(head_overview.slice_end_ratio < 0.99);

    assert_eq!(stitcher.crop_tail(40), 40);
    let back_to_tail = stitcher.get_or_render_thumbnail_overview(64, 240, 48);
    assert!(back_to_tail.is_cropped_window);
    assert!((back_to_tail.slice_end_ratio - 1.0).abs() < 1e-4);
    assert!(back_to_tail.slice_start_ratio > 0.01);
}

#[test]
fn test_periodic_repeating_rows_with_jitter_rejected_by_harmonic_guard() {
    let options = StitchOptions::default();
    let width = 200u32;
    let height = 340u32;
    let period = 24u32;

    let build_jittered_rows = |scroll: u32| -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            let doc_y = y + scroll;
            let row_idx = doc_y / period;
            let py = doc_y % period;
            // 基础条纹高度周期重复，仅叠加轻微奇偶行亮度微扰(±8)使 err(3*12)≈8 落入谐波陷阱
            let jitter = if row_idx.is_multiple_of(2) { 0u8 } else { 8 };
            let base = if py < 8 { 60 + jitter } else { 240 - jitter };
            for x in 0..width {
                let col_accent = if (x / 20).is_multiple_of(2) { 0 } else { 12 };
                img.put_pixel(
                    x,
                    y,
                    Rgba([base - col_accent, base, base - col_accent, 255]),
                );
            }
        }
        img
    };

    let f0 = build_jittered_rows(0);
    let f1 = build_jittered_rows(12);
    let res = estimate_shift(&f0, &f1, StitchDirection::Vertical, &options);
    assert!(
        !res.is_matched(options.confidence_threshold),
        "周期性重复行仅存在轻微微扰时，3 倍频谐波(+36)低误差陷阱应被多义性守卫拦截: {res:?}"
    );
}

#[test]
fn test_fluctuating_sticky_footer_and_header_keep_exact_geometry_without_duplication_or_skip() {
    let width = 80u32;
    let height = 200u32;
    // 用像素 R 通道编码唯一文档行号 (doc_y % 250)，便于精确校验接缝处是否吞行或重影
    let build_doc_frame = |scroll: u32, footer_h: u32| -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            let is_footer = y >= height - footer_h;
            for x in 0..width {
                if is_footer {
                    img.put_pixel(x, y, Rgba([10, 10, 10, 255]));
                } else {
                    let doc_y = y + scroll;
                    let r = (doc_y % 250) as u8;
                    img.put_pixel(x, y, Rgba([r, ((doc_y / 250) * 50) as u8, 128, 255]));
                }
            }
        }
        img
    };

    // 第 0 帧：scroll = 0, 底栏 20px
    let f0 = build_doc_frame(0, 20);
    let mut stitcher =
        LongCaptureStitcher::new(f0, StitchDirection::Vertical, StitchOptions::default());

    // 第 1 帧：下滚 +40px，底栏 20px
    let f1 = build_doc_frame(40, 20);
    let gen1 = stitcher.generation();
    let out1 = stitcher.apply_analyzed_shift(
        f1,
        gen1,
        FrameShiftAnalysis::Matched(MatchResult {
            offset: 40,
            confidence: 0.98,
            sticky_header_size: 0,
            sticky_footer_size: 20,
        }),
    );
    assert!(out1.is_stitched());
    assert_eq!(stitcher.dimensions(), (width, 240));

    // 第 2 帧：下滚 +30px，底栏突然膨胀到 45px（模拟多吞入 25px 纯白正文）
    let f2 = build_doc_frame(70, 45);
    let gen2 = stitcher.generation();
    let out2 = stitcher.apply_analyzed_shift(
        f2,
        gen2,
        FrameShiftAnalysis::Matched(MatchResult {
            offset: 30,
            confidence: 0.98,
            sticky_header_size: 0,
            sticky_footer_size: 45,
        }),
    );
    assert!(out2.is_stitched());
    assert_eq!(
        stitcher.dimensions(),
        (width, 270),
        "固定底栏膨胀时画布总高度增量仍应严格等于步长"
    );

    // 第 3 帧：下滚 +50px，底栏回缩到 15px
    let f3 = build_doc_frame(120, 15);
    let gen3 = stitcher.generation();
    let out3 = stitcher.apply_analyzed_shift(
        f3,
        gen3,
        FrameShiftAnalysis::Matched(MatchResult {
            offset: 50,
            confidence: 0.98,
            sticky_header_size: 0,
            sticky_footer_size: 15,
        }),
    );
    assert!(out3.is_stitched());
    assert_eq!(
        stitcher.dimensions(),
        (width, 320),
        "固定底栏回缩时画布总高度增量仍应严格等于步长"
    );

    // 验证正文每一行 doc_y (0 .. 320 - 15 = 305) 完全连续无跳行无重复
    let canvas = stitcher.canvas();
    for y in 0..(320 - 15) {
        let expected_r = (y % 250) as u8;
        assert_eq!(
            canvas.get_pixel(10, y).0[0],
            expected_r,
            "第 {y} 行文档内容发生错位或吞字"
        );
    }
}

#[test]
fn test_transient_sticky_footer_miss_still_peels_previous_footer_without_mid_canvas_artifact() {
    let width = 80u32;
    let height = 200u32;
    let footer_h = 30u32;
    let build_doc_frame = |scroll: u32| -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            let is_footer = y >= height - footer_h;
            for x in 0..width {
                if is_footer {
                    // 用特殊特征像素标识固定底栏，绝不应出现在画布正文中间
                    img.put_pixel(x, y, Rgba([255, 0, 255, 255]));
                } else {
                    let doc_y = y + scroll;
                    img.put_pixel(x, y, Rgba([(doc_y % 250) as u8, 80, 128, 255]));
                }
            }
        }
        img
    };

    let mut stitcher = LongCaptureStitcher::new(
        build_doc_frame(0),
        StitchDirection::Vertical,
        StitchOptions::default(),
    );

    // 第 1 帧：正常检出底栏 30px
    let gen1 = stitcher.generation();
    assert!(stitcher
        .apply_analyzed_shift(
            build_doc_frame(40),
            gen1,
            FrameShiftAnalysis::Matched(MatchResult {
                offset: 40,
                confidence: 0.98,
                sticky_header_size: 0,
                sticky_footer_size: footer_h,
            }),
        )
        .is_stitched());

    // 第 2 帧：模拟底栏内按钮闪烁导致本帧漏检返回 sticky_footer_size = 0
    let gen2 = stitcher.generation();
    assert!(stitcher
        .apply_analyzed_shift(
            build_doc_frame(80),
            gen2,
            FrameShiftAnalysis::Matched(MatchResult {
                offset: 40,
                confidence: 0.98,
                sticky_header_size: 0,
                sticky_footer_size: 0,
            }),
        )
        .is_stitched());

    // 第 3 帧：恢复检出底栏 30px
    let gen3 = stitcher.generation();
    assert!(stitcher
        .apply_analyzed_shift(
            build_doc_frame(120),
            gen3,
            FrameShiftAnalysis::Matched(MatchResult {
                offset: 40,
                confidence: 0.98,
                sticky_header_size: 0,
                sticky_footer_size: footer_h,
            }),
        )
        .is_stitched());

    assert_eq!(stitcher.dimensions(), (width, 320));
    let canvas = stitcher.canvas();
    for y in 0..(320 - footer_h) {
        let px = canvas.get_pixel(10, y).0;
        assert_ne!(
            px,
            [255, 0, 255, 255],
            "第 {y} 行出现固定底栏残留在正文中间"
        );
        assert_eq!(px[0], (y % 250) as u8, "第 {y} 行正文内容不连续");
    }
}

#[test]
fn test_seam_feathering_smooths_subtle_subpixel_brightness_step() {
    let width = 40u32;
    let height = 100u32;
    let f0 = RgbaImage::from_pixel(width, height, Rgba([100, 100, 100, 255]));
    let f1 = RgbaImage::from_pixel(width, height, Rgba([114, 114, 114, 255]));

    let mut stitcher =
        LongCaptureStitcher::new(f0, StitchDirection::Vertical, StitchOptions::default());
    let gen = stitcher.generation();
    assert!(stitcher
        .apply_analyzed_shift(
            f1,
            gen,
            FrameShiftAnalysis::Matched(MatchResult {
                offset: 30,
                confidence: 0.99,
                sticky_header_size: 0,
                sticky_footer_size: 0,
            }),
        )
        .is_stitched());

    let canvas = stitcher.canvas();
    // 接缝在 y=100，重叠羽化带位于 y=94..100，像素应从 100 单调平滑过渡到 114 而非硬跳变
    let before = canvas.get_pixel(10, 93).0[0];
    let mid = canvas.get_pixel(10, 96).0[0];
    let seam_prev = canvas.get_pixel(10, 99).0[0];
    let after = canvas.get_pixel(10, 100).0[0];
    assert_eq!(before, 100);
    assert_eq!(after, 114);
    assert!(
        before < mid && mid < seam_prev && seam_prev < after,
        "接缝重叠区应呈现单调递增的平滑羽化过渡: before={before}, mid={mid}, seam_prev={seam_prev}, after={after}"
    );
}

#[test]
fn test_periodic_table_with_unique_cell_markers_survives_pyramid_coarse_filtering() {
    let options = StitchOptions::default();
    let width = 360u32;
    let height = 380u32;
    let period = 18u32;

    let build_table_frame = |scroll: u32| -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            let doc_y = y + scroll;
            let row_idx = doc_y / period;
            let py = doc_y % period;
            let bg = if py == 0 {
                200u8
            } else if row_idx.is_multiple_of(2) {
                248
            } else {
                242
            };
            for x in 0..width {
                let mut val = bg;
                // 每行包含多列唯一单元格文本块，模拟真实数据表格的文本密度
                let col1 = 36 + ((row_idx * 37) % 80);
                let col2 = 150 + ((row_idx * 53) % 80);
                let col3 = 260 + ((row_idx * 71) % 50);
                if (4..14).contains(&py)
                    && ((x >= col1 && x < col1 + 44)
                        || (x >= col2 && x < col2 + 44)
                        || (x >= col3 && x < col3 + 36))
                    && (x + py * 2).is_multiple_of(3)
                {
                    val = 30;
                }
                img.put_pixel(x, y, Rgba([val, val, val, 255]));
            }
        }
        img
    };

    let f0 = build_table_frame(0);
    let f1 = build_table_frame(162); // 9 倍周期偏移 (9 * 18 = 162)
    let res = estimate_shift(&f0, &f1, StitchDirection::Vertical, &options);
    assert_eq!(
        res.offset, 162,
        "周期性表格大步长滚动不应在金字塔阶段漏波谷"
    );
    assert!(res.is_matched(options.confidence_threshold));
}
