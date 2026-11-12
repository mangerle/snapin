use super::*;
use image::{Rgba, RgbaImage};

fn make_textured_doc(width: u32, total_height: u32) -> RgbaImage {
    let mut img = RgbaImage::new(width, total_height);
    for y in 0..total_height {
        let band = (y / 14) as u8;
        for x in 0..width {
            let r = (band
                .wrapping_mul(37)
                .wrapping_add(((x / 5) as u8).wrapping_mul(19)))
                % 240
                + 10;
            let g = ((y.wrapping_mul(11).wrapping_add(x.wrapping_mul(7))) % 200) as u8 + 20;
            let b = ((y.wrapping_mul(23) ^ x.wrapping_mul(13)) % 200) as u8 + 30;
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    img
}

fn crop_vertical_frame(doc: &RgbaImage, y: u32, height: u32) -> RgbaImage {
    image::GenericImageView::view(doc, 0, y, doc.width(), height).to_image()
}

#[test]
fn test_paged_canvas_multi_chunk_roundtrip_and_partial_slicing() {
    // 生成跨越多个 512px 分块的真实纹理长文档，验证分块写入/前插/裁剪/整图物化逐像素完全一致。
    let doc = make_textured_doc(160, 1400);
    let mut canvas = super::paged_canvas::PagedCanvas::new(
        crop_vertical_frame(&doc, 300, 320),
        StitchDirection::Vertical,
    );
    // 尾部按小步长追加跨越 512 边界
    canvas.append_contiguous(&crop_vertical_frame(&doc, 620, 260));
    canvas.append_contiguous(&crop_vertical_frame(&doc, 880, 520));
    // 首部按多块前插
    canvas.prepend_contiguous(&crop_vertical_frame(&doc, 100, 200));
    canvas.prepend_contiguous(&crop_vertical_frame(&doc, 0, 100));

    assert_eq!(canvas.dimensions(), (160, 1400));
    assert_eq!(canvas.as_contiguous(), &doc);

    // 验证跨分块边界局部复制与像素读写
    let region = canvas.copy_region(20, 480, 90, 120);
    assert_eq!(
        region,
        image::GenericImageView::view(&doc, 20, 480, 90, 120).to_image()
    );

    // 验证头/尾跨越 512px 分块边界弹出
    let popped_head = canvas.pop_head(150).expect("头弹出应返回被截片段");
    assert_eq!(popped_head, crop_vertical_frame(&doc, 0, 150));
    let popped_tail = canvas.pop_tail(600).expect("尾弹出应返回被截片段");
    assert_eq!(popped_tail, crop_vertical_frame(&doc, 800, 600));
    assert_eq!(canvas.as_contiguous(), &crop_vertical_frame(&doc, 150, 650));
}

#[test]
fn test_stitcher_limit_reached_protects_memory_and_preserves_state() {
    let doc = make_textured_doc(180, 700);
    let frame0 = crop_vertical_frame(&doc, 0, 220);
    let opts = StitchOptions {
        max_canvas_extent: 320,
        max_canvas_pixels: 0,
        ..Default::default()
    };
    let mut stitcher = LongCaptureStitcher::new(frame0, StitchDirection::Vertical, opts);

    // 第一步 220 + 80 = 300 <= 320 应正常拼接
    let frame1 = crop_vertical_frame(&doc, 80, 220);
    assert!(matches!(
        stitcher.feed_frame_outcome(&frame1),
        StitchFrameOutcome::Stitched { offset: 80, .. }
    ));
    assert_eq!(stitcher.dimensions(), (180, 300));
    let gen_before_limit = stitcher.generation();

    // 第二步 300 + 80 = 380 > 320 必须触发 LimitReached 且完全保留原画布与基准帧
    let frame2 = crop_vertical_frame(&doc, 160, 220);
    assert_eq!(
        stitcher.feed_frame_outcome(&frame2),
        StitchFrameOutcome::LimitReached
    );
    assert_eq!(stitcher.dimensions(), (180, 300));
    assert_eq!(stitcher.generation(), gen_before_limit);
    assert_eq!(stitcher.last_frame(), &frame1);

    // 验证像素上限保护同样生效
    let pixel_opts = StitchOptions {
        max_canvas_extent: 0,
        max_canvas_pixels: 180 * 310,
        ..Default::default()
    };
    let mut px_stitcher = LongCaptureStitcher::new(
        crop_vertical_frame(&doc, 0, 220),
        StitchDirection::Vertical,
        pixel_opts,
    );
    assert!(px_stitcher.feed_frame_outcome(&frame1).is_stitched());
    assert_eq!(
        px_stitcher.feed_frame_outcome(&frame2),
        StitchFrameOutcome::LimitReached
    );
}

#[test]
fn test_async_analyze_and_apply_analyzed_shift_guards_generation() {
    let doc = make_textured_doc(160, 500);
    let f0 = crop_vertical_frame(&doc, 0, 220);
    let f1 = crop_vertical_frame(&doc, 90, 220);
    let f2 = crop_vertical_frame(&doc, 180, 220);

    let mut stitcher =
        LongCaptureStitcher::new(f0, StitchDirection::Vertical, StitchOptions::default());
    let g0 = stitcher.generation();
    let analysis1 = analyze_frame_shift(
        stitcher.last_frame(),
        &f1,
        stitcher.direction(),
        stitcher.options(),
    );
    assert!(matches!(
        analysis1,
        FrameShiftAnalysis::Matched(res) if (res.offset - 90).abs() <= 2
    ));

    // 错误代际必须被安全忽略
    assert_eq!(
        stitcher.apply_analyzed_shift(f1.clone(), g0.wrapping_add(99), analysis1),
        StitchFrameOutcome::Ignored
    );
    // 正确代际成功提交并递增代际
    assert!(stitcher
        .apply_analyzed_shift(f1, g0, analysis1)
        .is_stitched());
    assert!(stitcher.generation() > g0);

    // 基于最新代际继续推进与后台分块快照缩略图一致
    let g1 = stitcher.generation();
    let analysis2 = analyze_frame_shift(
        stitcher.last_frame(),
        &f2,
        stitcher.direction(),
        stitcher.options(),
    );
    assert!(stitcher
        .apply_analyzed_shift(f2, g1, analysis2)
        .is_stitched());
    let spec = stitcher.build_thumbnail_spec(80, 120, 48);
    let overview = render_thumbnail_overview(&stitcher.thumbnail_snapshot(), spec);
    assert!(overview.image.width() > 0 && overview.image.height() > 0);
}

#[test]
fn test_paged_canvas_write_cursor_cross_chunk_vertical_and_horizontal() {
    use super::paged_canvas::{put_axis_pixel, PagedCanvas, CHUNK_MAIN_SPAN};
    use image::{Rgba, RgbaImage};

    for direction in [StitchDirection::Vertical, StitchDirection::Horizontal] {
        let total_main = CHUNK_MAIN_SPAN * 2 + 40;
        let cross = 32;
        let base = match direction {
            StitchDirection::Vertical => {
                RgbaImage::from_pixel(cross, total_main, Rgba([1, 2, 3, 255]))
            }
            StitchDirection::Horizontal => {
                RgbaImage::from_pixel(total_main, cross, Rgba([1, 2, 3, 255]))
            }
        };
        let mut canvas = PagedCanvas::new(base, direction);
        // 先触发物化缓存生成
        let _cached = canvas.as_contiguous();

        // 从尾部跨分片连续写，并回跨至前序分片验证游标回溯与缓存失效
        let targets = [CHUNK_MAIN_SPAN + 10, CHUNK_MAIN_SPAN * 2 + 5, 20];
        {
            let mut cursor = canvas.write_cursor();
            for &m in &targets {
                let (chunk, local_m, dir) =
                    cursor.locate_row_mut(m).expect("合法主轴行应命中对应分片");
                put_axis_pixel(chunk, local_m, 7, dir, [250, 120, 30, 255]);
            }
        }

        let updated = canvas.as_contiguous();
        for &m in &targets {
            let px = match direction {
                StitchDirection::Vertical => updated.get_pixel(7, m).0,
                StitchDirection::Horizontal => updated.get_pixel(m, 7).0,
            };
            assert_eq!(px, [250, 120, 30, 255]);
        }
    }
}
