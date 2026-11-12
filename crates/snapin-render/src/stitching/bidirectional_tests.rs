//! 自适应位移上界与双向（向上/向左前插）长截图拼接单元测试。

use super::stitcher::LongCaptureStitcher;
use super::types::{StitchDirection, StitchFrameOutcome, StitchOptions};
use image::{Rgba, RgbaImage};

fn create_vertical_doc(width: u32, height: u32, scroll_y: u32) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    for y in 0..height {
        let doc_y = y + scroll_y;
        for x in 0..width {
            let mut seed = (u64::from(x) << 32) | u64::from(doc_y);
            seed ^= seed >> 30;
            seed = seed.wrapping_mul(0xbf58_476d_1ce4_e5b9);
            seed ^= seed >> 27;
            seed = seed.wrapping_mul(0x94d0_49bb_1331_11eb);
            seed ^= seed >> 31;
            img.put_pixel(
                x,
                y,
                Rgba([seed as u8, (seed >> 8) as u8, (seed >> 16) as u8, 255]),
            );
        }
    }
    img
}

fn create_horizontal_doc(width: u32, height: u32, scroll_x: u32) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let doc_x = x + scroll_x;
            let mut seed = (u64::from(doc_x) << 32) | u64::from(y);
            seed ^= seed >> 30;
            seed = seed.wrapping_mul(0xbf58_476d_1ce4_e5b9);
            seed ^= seed >> 27;
            seed = seed.wrapping_mul(0x94d0_49bb_1331_11eb);
            seed ^= seed >> 31;
            img.put_pixel(
                x,
                y,
                Rgba([seed as u8, (seed >> 8) as u8, (seed >> 16) as u8, 255]),
            );
        }
    }
    img
}

#[test]
fn test_adaptive_step_exceeds_legacy_300_and_reverse_80_caps() {
    // 720px 高画布：允许超过旧 300px 上限（+420px）以及超过旧 80px 上限的反向回滚（-220px）。
    let width = 180u32;
    let height = 720u32;
    let f0 = create_vertical_doc(width, height, 0);
    let f1 = create_vertical_doc(width, height, 420);
    let f2 = create_vertical_doc(width, height, 200);

    let mut stitcher =
        LongCaptureStitcher::new(f0, StitchDirection::Vertical, StitchOptions::default());

    let out1 = stitcher.feed_frame_outcome(&f1);
    assert_eq!(
        out1,
        StitchFrameOutcome::Stitched {
            offset: 420,
            confidence: 1.0,
            sticky_header_size: 0,
            sticky_footer_size: 0,
        },
        "自适应步长必须接纳高屏单帧 +420px 大幅度滚动"
    );
    assert_eq!(stitcher.dimensions(), (180, 1140));
    assert_eq!(stitcher.growth_polarity(), Some(1));

    let out2 = stitcher.feed_frame_outcome(&f2);
    assert_eq!(
        out2,
        StitchFrameOutcome::Stitched {
            offset: -220,
            confidence: 1.0,
            sticky_header_size: 0,
            sticky_footer_size: 0,
        },
        "反向回滚搜索上界应与正向对称，不再硬截断于 80px"
    );
    assert_eq!(stitcher.dimensions(), (180, 920));
}

#[test]
fn test_upward_prepending_with_sticky_header_and_floating_overlay_healing() {
    let width = 160u32;
    let height = 240u32;
    let header = 20u32;
    let footer = 20u32;
    // 悬浮按钮位于上半正文（x=116..140, y=56..80），向上滚出后下帧应能把上一帧的按钮残影治愈。
    let (btn_x0, btn_x1, btn_y0, btn_y1) = (116u32, 140u32, 56u32, 80u32);

    let build_frame = |scroll: u32| -> RgbaImage {
        let mut img = create_vertical_doc(width, height, scroll);
        for y in 0..header {
            for x in 0..width {
                let s = ((x / 8) % 2) as u8 * 50;
                img.put_pixel(x, y, Rgba([25 + s, 60 + (y as u8) * 3, 180, 255]));
            }
        }
        for y in (height - footer)..height {
            let rel_y = y - (height - footer);
            for x in 0..width {
                let s = ((x / 12) % 2) as u8 * 45;
                img.put_pixel(x, y, Rgba([180 - s, 50 + (rel_y as u8) * 4, 80, 255]));
            }
        }
        for y in btn_y0..btn_y1 {
            for x in btn_x0..btn_x1 {
                img.put_pixel(x, y, Rgba([245, 55, 45, 255]));
            }
        }
        img
    };

    // 从正文中间 scroll=240 出发，连续两次向上滚动 80px（scroll=160 -> scroll=80）。
    let mut stitcher = LongCaptureStitcher::new(
        build_frame(240),
        StitchDirection::Vertical,
        StitchOptions::default(),
    );

    let out1 = stitcher.feed_frame_outcome(&build_frame(160));
    match out1 {
        StitchFrameOutcome::Stitched {
            offset,
            sticky_header_size,
            sticky_footer_size,
            ..
        } => {
            assert_eq!(offset, -80);
            assert!(sticky_header_size >= 16);
            assert!(sticky_footer_size >= 16);
        }
        other => panic!("期望 Stitched，实际 {other:?}"),
    }
    assert_eq!(stitcher.growth_polarity(), Some(-1));
    assert_eq!(stitcher.dimensions(), (160, 320));

    // 首端唯一固定页眉完整保留，新增前插正文准确落于 header 下方，视口实时映射在顶部 start=0.0。
    let vp = stitcher.current_viewport_window();
    assert_eq!(vp.start, 0.0);
    assert_eq!(vp.size, 240.0);
    assert_eq!(vp.total, 320.0);

    // 验证原本被悬浮按钮遮住的 doc 区域在画布内（偏移 +step=80 之后）已被 clean backfill 抹平恢复。
    let clean_anchor = create_vertical_doc(width, height, 240);
    let healed_canvas_y0 = btn_y0 + 80;
    let healed_canvas_y1 = btn_y1 + 80;
    for cy in healed_canvas_y0..healed_canvas_y1 {
        let ref_y = cy - 80;
        for cx in btn_x0..btn_x1 {
            assert_eq!(
                stitcher.canvas().get_pixel(cx, cy),
                clean_anchor.get_pixel(cx, ref_y),
                "前插画布中位置 ({cx}, {cy}) 的悬浮残影应被干净正文回填修复"
            );
        }
    }

    // 继续二次向上前插 80px -> 总高度 400px，测试极端槽位比下缩略图窗口锚定顶部 `[0.0 .. ratio]`。
    assert!(stitcher.feed_frame_outcome(&build_frame(80)).is_stitched());
    assert_eq!(stitcher.dimensions(), (160, 400));
    let tall_overview = stitcher
        .get_or_render_thumbnail_overview(80, 100, 48)
        .clone();
    assert!(tall_overview.is_cropped_window);
    assert_eq!(tall_overview.slice_start_ratio, 0.0);
    assert!(tall_overview.slice_end_ratio < 0.9);
}

#[test]
fn test_leftward_horizontal_prepending_and_viewport() {
    let width = 200u32;
    let height = 100u32;
    let mut stitcher = LongCaptureStitcher::new(
        create_horizontal_doc(width, height, 180),
        StitchDirection::Horizontal,
        StitchOptions::default(),
    );

    let out = stitcher.feed_frame_outcome(&create_horizontal_doc(width, height, 110));
    assert!(matches!(
        out,
        StitchFrameOutcome::Stitched { offset: -70, .. }
    ));
    assert_eq!(stitcher.growth_polarity(), Some(-1));
    assert_eq!(stitcher.dimensions(), (270, 100));

    let vp = stitcher.current_viewport_window();
    assert_eq!(vp.start, 0.0);
    assert_eq!(vp.total, 270.0);

    let expected_left = create_horizontal_doc(width, height, 110);
    assert_eq!(
        stitcher.canvas().get_pixel(15, 40),
        expected_left.get_pixel(15, 40)
    );
}

#[test]
fn test_bidirectional_lock_unlock_and_auto_crop_reverse_false_semantics() {
    let width = 120u32;
    let height = 180u32;
    let f_mid = create_vertical_doc(width, height, 120);
    let f_up = create_vertical_doc(width, height, 60);
    let f_down = create_vertical_doc(width, height, 180);

    // 1. 即使关闭反向自动裁剪，从初始状态首次向上滚动依然应解锁并前插生长，锁定极性为 -1。
    let mut stitcher = LongCaptureStitcher::new(
        f_mid.clone(),
        StitchDirection::Vertical,
        StitchOptions {
            auto_crop_reverse: false,
            ..StitchOptions::default()
        },
    );
    assert!(matches!(
        stitcher.feed_frame_outcome(&f_up),
        StitchFrameOutcome::Stitched { offset: -60, .. }
    ));
    assert_eq!(stitcher.growth_polarity(), Some(-1));
    assert_eq!(stitcher.dimensions(), (120, 240));

    // 2. 极性锁定为 -1 后，关闭自动裁剪时再向下反滚返回 f_mid 应被忽略。
    assert_eq!(
        stitcher.feed_frame_outcome(&f_mid),
        StitchFrameOutcome::Ignored
    );
    assert_eq!(stitcher.dimensions(), (120, 240));

    // 3. 开启自动裁剪后向下回滚至 f_mid：应裁除首部回退至初始单帧大小并解锁极性，随后继续向下即顺畅转入 Forward(+1)。
    stitcher.set_options(StitchOptions::default());
    assert!(matches!(
        stitcher.feed_frame_outcome(&f_mid),
        StitchFrameOutcome::Stitched { offset: 60, .. }
    ));
    assert_eq!(stitcher.dimensions(), (120, 180));
    assert_eq!(stitcher.growth_polarity(), None);

    assert!(matches!(
        stitcher.feed_frame_outcome(&f_down),
        StitchFrameOutcome::Stitched { offset: 60, .. }
    ));
    assert_eq!(stitcher.growth_polarity(), Some(1));
    assert_eq!(stitcher.dimensions(), (120, 240));
}

#[test]
fn test_crop_head_on_backward_stitcher_syncs_anchor_and_pauses_below_frame() {
    let width = 120u32;
    let height = 160u32;
    let f_start = create_vertical_doc(width, height, 120);
    let f_up = create_vertical_doc(width, height, 40); // height = 240
    let f_up_more = create_vertical_doc(width, height, 40); // crop 后用于续接检查

    let mut stitcher =
        LongCaptureStitcher::new(f_start, StitchDirection::Vertical, StitchOptions::default());
    assert!(stitcher.feed_frame_outcome(&f_up).is_stitched());
    assert_eq!(stitcher.dimensions(), (120, 240));

    // 上裁剪 40px -> 剩余 200 >= 160：激活锚点在上端 [0..160]，即 doc scroll=80。
    assert_eq!(stitcher.crop_head(40), 40);
    assert!(!stitcher.is_anchor_truncated_below_frame());
    assert_eq!(stitcher.growth_polarity(), Some(-1));
    assert_eq!(
        stitcher.last_frame(),
        &create_vertical_doc(width, height, 80)
    );

    // 继续向上滚动 -40px（doc scroll=40）无缝衔接。
    assert!(matches!(
        stitcher.feed_frame_outcome(&f_up_more),
        StitchFrameOutcome::Stitched { offset: -40, .. }
    ));
    assert_eq!(stitcher.dimensions(), (120, 240));

    // 若进一步裁短到单帧以下，立即锁定保护并允许撤回无损恢复。
    assert_eq!(stitcher.crop_head(120), 120);
    assert!(stitcher.is_anchor_truncated_below_frame());
    assert_eq!(stitcher.dimensions(), (120, 120));
    assert!(stitcher.undo_crop());
    assert!(!stitcher.is_anchor_truncated_below_frame());
    assert_eq!(stitcher.dimensions(), (120, 240));
}
