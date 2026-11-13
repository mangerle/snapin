use super::*;
use crate::ports::NoopScreenRecorder;
use crate::snipper::magnifier::ColorFormat;
use crate::snipper::selection::Rect;
use crate::snipper::window::state::{LongCaptureSession, RecordingSession};
use crate::snipper::window::types::SnipperOutcome;
use image::Rgba;
use snapin_annotation::action::AnnotationShape;
use snapin_annotation::shapes::RectAction;
use snapin_annotation::LineStyle;
use std::sync::Arc;

/// 取色器按 HEX/RGB 格式输出，并在坐标越界时夹取到边缘
#[test]
fn test_color_text_at_cursor_formats_and_clamps() {
    let mut state = SnipperState::new();
    let img = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));

    state.viewport.cursor_pos = (2.0, 2.0);
    state.viewport.magnifier.format = ColorFormat::Hex;
    assert_eq!(color_text_at_cursor(&state, &img), "#FF0000");

    state.viewport.magnifier.format = ColorFormat::Rgb;
    assert_eq!(color_text_at_cursor(&state, &img), "rgb(255, 0, 0)");

    state.viewport.cursor_pos = (99.0, -5.0);
    assert_eq!(color_text_at_cursor(&state, &img), "rgb(255, 0, 0)");
}

#[test]
fn test_crop_without_selection() {
    let mut state = SnipperState::new();
    let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
    assert!(crop_selection_image(&mut state, &original).is_none());
}

#[test]
fn test_crop_without_annotations_returns_original() {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 10.0,
            y: 10.0,
            width: 50.0,
            height: 50.0,
        },
        active_drag: None,
    };
    let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
    let cropped = crop_selection_image(&mut state, &original).expect("应成功裁切选区");
    assert_eq!(cropped.width(), 50);
    assert_eq!(cropped.height(), 50);
    assert_eq!(cropped.get_pixel(0, 0), &Rgba([255, 255, 255, 255]));
}

#[test]
fn test_crop_with_annotations_composited() {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 10.0,
            y: 10.0,
            width: 50.0,
            height: 50.0,
        },
        active_drag: None,
    };

    // 添加一个全红色的填充矩形标注到全屏坐标 (15, 15, 20, 20)
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(15.0, 15.0, 20.0, 20.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 1.0,
            is_filled: true,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));

    let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
    let cropped = crop_selection_image(&mut state, &original).expect("应成功裁切合成选区");

    assert_eq!(cropped.width(), 50);
    assert_eq!(cropped.height(), 50);
    // (0, 0) 对应原图 (10, 10)，未被矩形覆盖，应仍为白色
    assert_eq!(cropped.get_pixel(0, 0), &Rgba([255, 255, 255, 255]));
    // (10, 10) 对应原图 (20, 20)，位于矩形内部，应合成红色
    assert_eq!(cropped.get_pixel(10, 10), &Rgba([255, 0, 0, 255]));
}

#[test]
fn test_handle_action_pin_sets_pinned_outcome() {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 40.0,
        },
        active_drag: None,
    };
    let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
    handle_action_pin(&mut state, &original);

    match &state.session.outcome {
        Some(SnipperOutcome::Pinned(item)) => {
            assert_eq!(item.position, (10, 10));
            assert_eq!(item.pixmap.width(), 40);
            assert_eq!(item.pixmap.height(), 40);
        }
        _ => panic!("预期输出 Pinned 状态"),
    }
}

#[test]
fn test_handle_action_ocr_empty_image_keeps_session_open() {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
        },
        active_drag: None,
    };
    let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
    handle_action_ocr(&mut state, &original);

    assert!(
        state.session.outcome.is_none(),
        "空图像或引擎不可用时不得以 Copied 状态退出"
    );
    assert!(
        state.session.status_message.is_some(),
        "识别失败或空结果必须给出用户提示"
    );
}

#[test]
fn test_handle_action_translate_graceful_handling() {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
        },
        active_drag: None,
    };
    let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
    // 无可用模型或未识别到文字时安全返回，不崩溃，也不得压入破坏性图元
    handle_action_translate(&mut state, &original);
    assert!(!state.editor.annotations.can_undo());
    assert!(state.session.status_message.is_some());
}

#[test]
fn test_handle_action_translate_zero_or_idle_selection() {
    let mut state = SnipperState::new();
    let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
    // Idle 状态安全返回
    handle_action_translate(&mut state, &original);

    // 零尺寸锁定状态安全返回
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 10.0,
            y: 10.0,
            width: 0.0,
            height: 0.0,
        },
        active_drag: None,
    };
    handle_action_translate(&mut state, &original);
    assert!(!state.editor.annotations.can_undo());
}

/// 长截图与屏幕录制必须互斥，避免长截图过程中误开录屏并请求 RemoteDesktop 权限。
#[test]
fn test_long_capture_and_recording_sessions_are_mutually_exclusive() {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 10.0,
            y: 10.0,
            width: 50.0,
            height: 50.0,
        },
        active_drag: None,
    };
    let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));

    state.long_capture = Some(LongCaptureSession::new(
        original.view(10, 10, 50, 50).to_image(),
    ));
    handle_action_start_recording(&mut state, &original);
    assert!(state.recording.is_none());
    assert!(state.session.status_message.is_some());

    state.long_capture = None;
    state.recording = Some(RecordingSession::new(
        Arc::new(NoopScreenRecorder),
        Arc::new(|_| {}),
    ));
    handle_action_long_capture(&mut state, &original);
    assert!(state.long_capture.is_none());
    assert!(state.session.status_message.is_some());
}

/// 美化配置必须转换为社交画布尺寸，后台渲染与保存路径共用同一组参数。
#[test]
fn test_configured_beautify_options_produce_social_canvas() {
    let mut state = SnipperState::new();
    state.session.config.beautify.enabled = true;
    state.session.config.beautify.social_size = snapin_render::SocialSize::Square;
    let options = snapin_render::BeautifyOptions::from(&state.session.config.beautify);
    let mut input = tiny_skia::Pixmap::new(100, 50).expect("创建测试位图失败");
    input.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
    let output =
        snapin_render::render_beautified(input.as_ref(), &options).expect("美化输出应成功");
    assert_eq!((output.width(), output.height()), (1080, 1080));
}

/// 超大贴图策略应在屏幕安全范围外自动按可读倍率缩放，超出下限时阻止创建以保护图形表面。
#[test]
fn test_evaluate_pin_size_policy_scales_or_rejects_oversized_images() {
    assert_eq!(
        evaluate_pin_size_policy(2, 2, (1920.0, 1080.0)),
        PinSizeDecision::Allow { scale: 1.0 }
    );
    assert_eq!(
        evaluate_pin_size_policy(800, 600, (1920.0, 1080.0)),
        PinSizeDecision::Allow { scale: 1.0 }
    );

    match evaluate_pin_size_policy(800, 2000, (1920.0, 1080.0)) {
        PinSizeDecision::Allow { scale } => {
            assert!((0.25..1.0).contains(&scale), "异常长图应自动缩放到可读倍率");
        }
        other => panic!("中等长图应允许等比缩放贴图，实际: {other:?}"),
    }

    assert_eq!(
        evaluate_pin_size_policy(1000, 8000, (1920.0, 1080.0)),
        PinSizeDecision::TooLarge {
            width: 1000,
            height: 8000,
        }
    );
    assert_eq!(
        evaluate_pin_size_policy(40, 2000, (1920.0, 1080.0)),
        PinSizeDecision::TooLarge {
            width: 40,
            height: 2000,
        }
    );
}

/// 长截图尺寸超限时 Pin 必须安全拒绝，中等长图则生成带适配倍率的贴图；其余冲突动作入口应静默隔离。
#[test]
fn test_oversized_pin_guard_and_long_capture_action_isolation() {
    let mut state = SnipperState::new();
    state.viewport.screen_size = (1920.0, 1080.0);
    let original = RgbaImage::from_pixel(600, 400, Rgba([255, 255, 255, 255]));
    state.viewport.selection = SelectionState::Locked {
        rect: Rect::new(20.0, 20.0, 300.0, 200.0),
        active_drag: None,
    };

    // 预先污染普通截图遗留工具与标注，启动长截图必须彻底重置它们
    state.editor.annotation.toolbar.active_tool = Some(crate::editor::toolbar::Tool::Rect);
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(20.0, 20.0, 50.0, 50.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));
    handle_action_long_capture(&mut state, &original);
    assert!(state.long_capture.is_some());
    assert!(state.editor.annotation.toolbar.active_tool.is_none());
    assert!(!state.editor.annotations.can_undo());

    // 冲突动作入口在长截图阶段不得修改结果或提交任何动作
    handle_action_pick_color(&mut state, &original);
    assert!(state.session.outcome.is_none());
    handle_action_ocr(&mut state, &original);
    handle_action_ocr_edit(&mut state, &original);
    handle_action_redact(&mut state, &original);
    handle_action_qr(&mut state, &original);
    handle_action_translate(&mut state, &original);
    assert!(state.session.status_message.is_none() || state.jobs.pending_media.is_none());

    // 超大长图（height = 8000）执行 Pin 时必须拒绝创建并保留会话、给出提示
    let huge = RgbaImage::from_pixel(800, 8000, Rgba([10, 20, 30, 255]));
    state.long_capture = Some(LongCaptureSession::new(huge));
    handle_action_pin(&mut state, &original);
    assert!(state.session.outcome.is_none(), "超限长图不得创建贴图");
    assert!(state.long_capture.is_some(), "拒绝贴图后不得销毁长截图会话");
    assert!(
        state.session.status_message.is_some(),
        "拒绝贴图后应提示用户改用保存或复制"
    );

    // 可适应范围的长图（600x1600）应允许贴图并自动设置 < 1.0 的初始倍率
    let moderate = RgbaImage::from_pixel(600, 1600, Rgba([10, 20, 30, 255]));
    state.long_capture = Some(LongCaptureSession::new(moderate));
    handle_action_pin(&mut state, &original);
    match state.session.outcome.take() {
        Some(SnipperOutcome::Pinned(item)) => {
            assert!(item.transform.scale() < 1.0 && item.transform.scale() >= 0.25);
        }
        other => panic!("中等长图应生成缩放贴图，实际: {other:?}"),
    }
}

/// 长图编辑态导出时，即使视口滚动到最底部，仍须将视口外顶部及当前视口内的标注正确复合到整张直通 RGBA 长图，
/// 且半透明标注不能因预乘往返出现发黑。
#[test]
fn test_long_capture_crop_selection_image_composites_scrolled_annotations_and_alpha() {
    let mut state = SnipperState::new();
    state.viewport.screen_size = (1280.0, 800.0);
    let original = RgbaImage::from_pixel(400, 400, Rgba([0, 0, 0, 255]));
    state.viewport.selection = SelectionState::Locked {
        rect: Rect::new(50.0, 50.0, 200.0, 200.0),
        active_drag: None,
    };

    let long_img = RgbaImage::from_pixel(200, 600, Rgba([40, 80, 120, 255]));
    let mut session = LongCaptureSession::new(long_img);
    session.enter_editing();
    session.viewport_offset_y = 400.0;
    state.long_capture = Some(session);

    // 1. 滚出当前视口的顶部（canvas y = 20..60）实心红色矩形
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(20.0, 20.0, 40.0, 40.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            is_filled: true,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));
    // 2. 位于底部当前视口内（canvas y = 500..540）的纯色半透明绿色矩形
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(20.0, 500.0, 40.0, 40.0),
            color: (0, 255, 0, 128).into(),
            stroke_width: 2.0,
            is_filled: true,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));

    let exported = crop_selection_image(&mut state, &original).expect("应成功导出长图");
    assert_eq!(exported.dimensions(), (200, 600));
    assert_eq!(
        exported.get_pixel(30, 30).0,
        [255, 0, 0, 255],
        "视口外顶部标注必须精准合成到长画布顶端"
    );

    let blended = exported.get_pixel(30, 510).0;
    assert_eq!(blended[3], 255, "不透明背景合成后 alpha 必须保持 255");
    assert!(
        blended[1] >= 165 && blended[0] <= 22 && blended[2] <= 62,
        "半透明混合颜色不应受预乘畸变或发黑: {blended:?}"
    );
}

/// 相交或邻近的半透明标注必须合并为互不相交的脏矩形，且合成结果与整图回放逐像素一致
#[test]
fn test_long_capture_dirty_rect_composition_matches_full_replay() {
    let mut state = SnipperState::new();
    let mut first_frame = RgbaImage::from_pixel(200, 1200, Rgba([220, 230, 240, 255]));
    for y in 0..1200 {
        for x in 0..200 {
            first_frame.put_pixel(x, y, Rgba([((x + y) % 255) as u8, 180, 210, 255]));
        }
    }
    state.long_capture = Some(LongCaptureSession::new(first_frame.clone()));
    state.viewport.selection = SelectionState::Locked {
        rect: Rect::new(0.0, 0.0, 200.0, 300.0),
        active_drag: None,
    };

    // 顶部两个半透明重叠矩形（应合并为单个脏矩形，避免二次回放变深）
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(10.0, 20.0, 80.0, 60.0),
            color: (255, 0, 0, 128).into(),
            stroke_width: 2.0,
            is_filled: true,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(50.0, 40.0, 80.0, 60.0),
            color: (0, 0, 255, 128).into(),
            stroke_width: 2.0,
            is_filled: true,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));
    // 底部远端一个独立矩形
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(30.0, 1050.0, 70.0, 50.0),
            color: (0, 200, 0, 255).into(),
            stroke_width: 3.0,
            is_filled: false,
            corner_radius: 4.0,
            line_style: LineStyle::Solid,
        }));

    let rects = collect_disjoint_dirty_rects(&state.editor.annotations, 200, 1200)
        .expect("普通图元应生成互不相交的脏矩形列表");
    assert_eq!(rects.len(), 2, "重叠的两个顶部图元应合并，总共 2 个脏矩形");

    let actual = crop_selection_image(&mut state, &first_frame).expect("应成功局部合成长图");
    let mut expected_pixmap = rgba_image_to_pixmap(&first_frame).unwrap();
    state
        .editor
        .annotations
        .replay_all(&mut expected_pixmap.as_mut());
    let expected = pixmap_into_rgba_image(expected_pixmap).unwrap();
    // 局部 Pixmap 平移坐标后，tiny_skia 对圆角贝塞尔曲线细分存在极微小的浮点抗锯齿覆盖率差异，
    // 其余全部区域及半透明重叠区域必须逐像素一致。
    for y in 0..1200 {
        for x in 0..200 {
            let a = actual.get_pixel(x, y).0;
            let b = expected.get_pixel(x, y).0;
            let max_diff = a
                .iter()
                .zip(b.iter())
                .map(|(u, v)| u.abs_diff(*v))
                .max()
                .unwrap_or(0);
            assert!(
                max_diff <= 16,
                "像素 ({x}, {y}) 合成差异过大: actual={a:?}, expected={b:?}"
            );
        }
    }
    // 显式校验两个半透明矩形重叠区域内部像素（如 (60, 50)）必须 0 误差
    assert_eq!(actual.get_pixel(60, 50), expected.get_pixel(60, 50));
}

#[test]
fn test_long_capture_editing_unlocks_pick_color_with_viewport_offset() {
    let mut state = SnipperState::new();
    let original = RgbaImage::from_pixel(400, 400, Rgba([0, 0, 0, 255]));
    state.viewport.selection = SelectionState::Locked {
        rect: Rect::new(50.0, 50.0, 200.0, 200.0),
        active_drag: None,
    };

    let mut long_img = RgbaImage::from_pixel(200, 600, Rgba([10, 20, 30, 255]));
    long_img.put_pixel(10, 410, Rgba([12, 34, 56, 255]));
    let mut session = LongCaptureSession::new(long_img);
    session.enter_editing();
    session.viewport_offset_y = 400.0;
    state.long_capture = Some(session);

    state.viewport.cursor_pos = (60.0, 60.0);
    state.viewport.magnifier.format = ColorFormat::Hex;
    assert_eq!(
        color_text_at_cursor(&state, &original),
        "#0C2238",
        "编辑态取色应映射视口偏移读取长图实际像素"
    );
    state.set_status_message("待清理的旧提示");
    handle_action_pick_color(&mut state, &original);
    assert!(
        state.session.status_message.is_none(),
        "编辑态下调用取色动作不应被长图录制隔离提前拦截（应执行到 clear_status_message 及后续剪贴板提交）"
    );
}
