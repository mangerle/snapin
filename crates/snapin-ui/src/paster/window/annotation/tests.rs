use super::*;
use crate::model::PasterItem;
use snapin_annotation::shapes::TranslationShapeItem;
use snapin_annotation::AnnotationShape;
use snapin_config::config::CoverMode;
use tiny_skia::{Color, Pixmap};

/// 构造一张指定尺寸、内容全透明的测试贴图状态
fn test_state(width: u32, height: u32) -> PasterWindowState {
    let pixmap = Pixmap::new(width, height).expect("创建测试位图失败");
    let item = PasterItem::new(1, "测试贴图".to_string(), pixmap, (0, 0));
    PasterWindowState::new(item)
}

/// 以矩形工具提交一笔填充矩形，返回提交后的可撤销图元
fn commit_filled_rect(
    state: &mut PasterWindowState,
    start: crate::geometry::Point,
    end: crate::geometry::Point,
    color: Color,
) -> AnnotationShape {
    state.interaction.annotation.toolbar.active_tool = Some(Tool::Rect);
    state.interaction.annotation.toolbar.sub_options.filled = true;
    state.interaction.annotation.toolbar.sub_options.color = color;
    state.interaction.annotation.stroke.points.clear();
    state.interaction.annotation.stroke.points.push(start);
    state.interaction.annotation.stroke.points.push(end);
    assert!(state.commit_pending_stroke(), "矩形标注应提交成功");
    state
        .document
        .annotations
        .undo_stack()
        .last()
        .cloned()
        .expect("提交后撤销栈应至少包含一个图元")
}

/// 缩放 1.0 与 1.6667 下屏幕坐标与图像坐标应正确互转
#[test]
fn 屏幕与图像坐标换算支持不同缩放() {
    assert_eq!(
        screen_to_image_point((100.0, 50.0), 1.0, 200, 100),
        (100.0, 50.0)
    );

    let (x, y) = screen_to_image_point((100.0, 100.0), 1.6667, 200, 100);
    assert!(
        (x - 59.9988).abs() < 0.01,
        "缩放 1.6667 时横坐标应约为 60，实际 {x}"
    );
    assert!(
        (y - 59.9988).abs() < 0.01,
        "缩放 1.6667 时纵坐标应约为 60，实际 {y}"
    );

    let screen = image_to_screen_point(Point::new(60.0, 30.0), 1.6667);
    let (sx, sy) = (screen.x, screen.y);
    assert!(
        (sx - 100.002).abs() < 0.01,
        "图像横坐标应按缩放放大，实际 {sx}"
    );
    assert!(
        (sy - 50.001).abs() < 0.01,
        "图像纵坐标应按缩放放大，实际 {sy}"
    );
}

/// 超出图像范围的屏幕坐标必须夹取到图像边界
#[test]
fn 坐标换算会夹取到图像边界() {
    assert_eq!(
        screen_to_image_point((-20.0, -10.0), 1.0, 200, 100),
        (0.0, 0.0)
    );
    assert_eq!(
        screen_to_image_point((900.0, 900.0), 1.0, 200, 100),
        (200.0, 100.0)
    );
    assert_eq!(
        screen_to_image_point((-5.0, 500.0), 1.6667, 200, 100),
        (0.0, 100.0)
    );
}

/// 图像内外判定应覆盖边界，并按缩放倍数换算宽高
#[test]
fn 图像内外判定遵循缩放后的边界() {
    assert!(is_point_inside_image((0.0, 0.0), 1.0, 200, 100));
    assert!(is_point_inside_image((200.0, 100.0), 1.0, 200, 100));
    assert!(!is_point_inside_image((200.5, 100.0), 1.0, 200, 100));
    assert!(is_point_inside_image((333.34, 166.67), 1.6667, 200, 100));
    assert!(!is_point_inside_image((333.35, 166.67), 1.6667, 200, 100));
}

/// 提交一笔后历史长度增加、图像像素被修改，而底图保持原始内容
#[test]
fn 提交一笔会写入图像并记录历史() {
    let mut state = test_state(100, 80);
    let original = state.document.item.pixmap.data().to_vec();

    state.interaction.annotation.toolbar.active_tool = Some(Tool::Pen);
    state
        .interaction
        .annotation
        .toolbar
        .sub_options
        .custom_stroke_width = 4.0;
    assert!(state.begin_stroke((10.0, 10.0)));
    assert!(state.update_stroke((80.0, 60.0)));
    assert!(state.commit_pending_stroke());

    assert_eq!(
        state.document.annotations.undo_stack().len(),
        1,
        "提交一笔后撤销栈长度应为 1"
    );
    assert_ne!(
        state.document.item.pixmap.data(),
        original.as_slice(),
        "提交一笔后展示图像必须被修改"
    );
    assert_eq!(
        state
            .document
            .base_pixmap
            .as_ref()
            .expect("标注后应有底图")
            .data(),
        original.as_slice(),
        "底图必须保持创建贴图时的原始像素"
    );
    assert!(
        state.interaction.annotation.stroke.is_empty(),
        "提交后待提交点集必须清空"
    );
}

/// 画两笔后撤销一次，图像应与「只画第一笔」的重建结果逐像素一致；重做后恢复两笔
#[test]
fn 撤销重做按底图重放标注() {
    let mut state = test_state(64, 64);

    let first_shape = commit_filled_rect(
        &mut state,
        crate::geometry::Point::new(4.0, 4.0),
        crate::geometry::Point::new(24.0, 24.0),
        Color::from_rgba8(239, 68, 68, 255),
    );
    commit_filled_rect(
        &mut state,
        crate::geometry::Point::new(32.0, 32.0),
        crate::geometry::Point::new(56.0, 56.0),
        Color::from_rgba8(59, 130, 246, 255),
    );
    let two_stroke_pixels = state.document.item.pixmap.data().to_vec();
    assert_eq!(state.document.annotations.undo_stack().len(), 2);

    assert!(state.undo_annotation(), "存在两笔标注时应允许撤销");
    assert_eq!(state.document.annotations.undo_stack().len(), 1);

    // 期望图：透明底图上仅重放第一笔
    let mut expected = Pixmap::new(64, 64).expect("创建期望位图失败");
    first_shape.draw(&mut expected.as_mut());
    assert_eq!(
        state.document.item.pixmap.data(),
        expected.data(),
        "撤销后图像应等价于只画第一笔"
    );

    assert!(state.redo_annotation(), "存在被撤销的标注时应允许重做");
    assert_eq!(state.document.annotations.undo_stack().len(), 2);
    assert_eq!(
        state.document.item.pixmap.data(),
        two_stroke_pixels.as_slice(),
        "重做后图像应恢复两笔标注"
    );
}

/// 贴图主栏必须包含复制、保存、关闭与整图识别/翻译动作，且不得混入截图专属按钮
#[test]
fn 贴图主栏包含动作按钮且排除截图专属项() {
    let expected = [
        ToolbarItem::Tool(Tool::Rect),
        ToolbarItem::Tool(Tool::Badge),
        ToolbarItem::Tool(Tool::Arrow),
        ToolbarItem::Tool(Tool::Pen),
        ToolbarItem::Tool(Tool::Marker),
        ToolbarItem::Tool(Tool::Mosaic),
        ToolbarItem::Tool(Tool::Magnifier),
        ToolbarItem::Tool(Tool::Spotlight),
        ToolbarItem::Tool(Tool::Eraser),
        ToolbarItem::Tool(Tool::Text),
        ToolbarItem::Action(ToolbarAction::Undo),
        ToolbarItem::Action(ToolbarAction::Redo),
        ToolbarItem::Action(ToolbarAction::Ocr),
        ToolbarItem::Action(ToolbarAction::Translate),
        ToolbarItem::Action(ToolbarAction::MouseThrough),
        ToolbarItem::Action(ToolbarAction::Close),
        ToolbarItem::Action(ToolbarAction::Save),
        ToolbarItem::Action(ToolbarAction::Copy),
    ];
    assert_eq!(PASTER_MAIN_ITEMS, &expected);
    assert!(
        PASTER_MAIN_ITEMS.windows(2).any(|pair| {
            pair == [
                ToolbarItem::Tool(Tool::Eraser),
                ToolbarItem::Tool(Tool::Text),
            ]
        }),
        "文字工具必须紧跟效果工具"
    );
    assert!(
        PASTER_MAIN_ITEMS.windows(2).any(|pair| {
            pair == [
                ToolbarItem::Tool(Tool::Text),
                ToolbarItem::Action(ToolbarAction::Undo),
            ]
        }),
        "文字工具必须位于撤销按钮之前"
    );
    assert!(
        PASTER_MAIN_ITEMS.windows(2).any(|pair| {
            pair == [
                ToolbarItem::Action(ToolbarAction::Redo),
                ToolbarItem::Action(ToolbarAction::Ocr),
            ]
        }),
        "文字识别按钮必须紧跟重做按钮"
    );
    assert!(
        PASTER_MAIN_ITEMS.windows(2).any(|pair| {
            pair == [
                ToolbarItem::Action(ToolbarAction::Ocr),
                ToolbarItem::Action(ToolbarAction::Translate),
            ]
        }),
        "翻译按钮必须紧跟文字识别按钮"
    );

    for action in [
        ToolbarItem::Action(ToolbarAction::Ocr),
        ToolbarItem::Action(ToolbarAction::Translate),
        ToolbarItem::Action(ToolbarAction::MouseThrough),
        ToolbarItem::Action(ToolbarAction::Close),
        ToolbarItem::Action(ToolbarAction::Save),
        ToolbarItem::Action(ToolbarAction::Copy),
    ] {
        assert!(
            PASTER_MAIN_ITEMS.contains(&action),
            "主栏应包含动作按钮 {action:?}"
        );
    }
    assert!(
        !PASTER_MAIN_ITEMS.contains(&ToolbarItem::Action(ToolbarAction::Pin)),
        "贴图窗口不应出现“贴图置顶”按钮"
    );
    assert!(
        PASTER_MAIN_ITEMS.contains(&ToolbarItem::Tool(Tool::Text)),
        "贴图主栏必须提供文字标注工具"
    );
}

/// 注入翻译图元后历史长度增加、展示图像被修改，撤销后恢复为原始底图
#[test]
fn 翻译结果注入可从底图撤销() {
    let mut state = test_state(120, 80);
    let original_item_pixels = state.document.item.pixmap.data().to_vec();
    assert!(
        state.document.base_pixmap.is_none(),
        "未产生标注时不应提前复制底图"
    );

    let items = vec![
        TranslationShapeItem {
            rect: Rect::new(10.0, 10.0, 60.0, 24.0),
            original: "Hello".to_string(),
            translated: "你好".to_string(),
            bg_color: (255, 0, 0, 255).into(),
            text_color: (0, 0, 0, 255).into(),
            font_size: 14.0,
            cover_mode: CoverMode::SolidColor,
        },
        TranslationShapeItem {
            rect: Rect::new(10.0, 44.0, 80.0, 24.0),
            original: "World".to_string(),
            translated: "世界".to_string(),
            bg_color: (0, 80, 255, 255).into(),
            text_color: (255, 255, 255, 255).into(),
            font_size: 14.0,
            cover_mode: CoverMode::SolidColor,
        },
    ];

    let injected = state.apply_translation_items(items);
    assert_eq!(injected, 2, "应报告注入了两段翻译");
    assert_eq!(
        state
            .document
            .base_pixmap
            .as_ref()
            .expect("产生标注后应捕获底图")
            .data(),
        original_item_pixels.as_slice(),
        "底图必须保持为标注前像素"
    );
    assert_eq!(
        state.document.annotations.undo_stack().len(),
        1,
        "整图翻译应作为一条复合标注进入历史"
    );
    assert_ne!(
        state.document.item.pixmap.data(),
        original_item_pixels.as_slice(),
        "翻译注入后展示图像必须与底图不同"
    );

    assert!(state.undo_annotation(), "翻译标注应支持撤销");
    assert_eq!(state.document.annotations.undo_stack().len(), 0);
    assert_eq!(
        state.document.item.pixmap.data(),
        original_item_pixels.as_slice(),
        "撤销翻译后必须精确恢复底图"
    );
}

/// 工具条所需尺寸在 1.0 与 1.67 倍率下按界面像素等比缩放
#[test]
fn 工具条所需尺寸随界面倍率缩放() {
    let mut toolbar = create_paster_toolbar();
    let mut baseline: Option<(f32, f32)> = None;
    for scale in [1.0f32, 1.67] {
        toolbar.ui_scale = scale;
        let main_w = toolbar.main_width();
        let main_h = toolbar.main_height();
        let (width, height) = toolbar_required_extent(scale, main_w, main_h);

        assert!(width >= main_w, "扩窗宽度必须容纳主栏：{width} < {main_w}");
        assert!(
            height >= main_h,
            "扩窗高度必须容纳主栏：{height} < {main_h}"
        );

        match baseline {
            None => baseline = Some((width, height)),
            Some((base_w, base_h)) => {
                let ratio = scale / 1.0;
                assert!(
                    (width / base_w - ratio).abs() < 0.01,
                    "宽度应按界面倍率线性缩放：{width} / {base_w} 不等于 {ratio}"
                );
                assert!(
                    (height / base_h - ratio).abs() < 0.01,
                    "高度应按界面倍率线性缩放：{height} / {base_h} 不等于 {ratio}"
                );
            }
        }
    }

    // 主栏较窄时窗口宽度由属性栏预留宽度兜底
    assert_eq!(toolbar_required_extent(1.0, 100.0, 40.0), (1276.0, 96.0));
}

/// 小贴图上工具条紧贴图像下方（仅 8 像素间距），属性栏浮在主栏上方
#[test]
fn 工具条锚定在图像下方() {
    let mut state = test_state(140, 80);
    state.viewport.ui_scale = 1.0;
    state.chrome.show_toolbar = true;
    state.interaction.annotation.toolbar.active_tool = Some(Tool::Rect);

    refresh_toolbar_layout(&mut state, 508.0, 180.0);

    assert_eq!(state.interaction.annotation.toolbar.main_rect.x, 8.0);
    assert_eq!(
        state.interaction.annotation.toolbar.main_rect.y, 88.0,
        "主栏应紧贴图像底边下方 8 像素"
    );
    let sub = state
        .interaction
        .annotation
        .toolbar
        .sub_rect
        .expect("矩形工具应展开属性栏");
    assert!(
        sub.y + sub.height <= state.interaction.annotation.toolbar.main_rect.y + 0.01,
        "属性栏应位于主栏上方，实际 y: {} 高: {}",
        sub.y,
        sub.height
    );
    assert!(sub.y >= 0.0, "属性栏不得超出窗口顶部，实际 y: {}", sub.y);
    assert!(
        state.interaction.annotation.toolbar.main_rect.y
            + state.interaction.annotation.toolbar.main_rect.height
            <= 180.0 - 8.0,
        "主栏底边应留出 8 像素下边距"
    );
}
