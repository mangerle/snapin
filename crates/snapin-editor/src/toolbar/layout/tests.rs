//! 二级属性栏布局与命中区域测试。

use super::*;

fn layout_for(
    main_rect: &Rect,
    active_tool: Option<Tool>,
    screen: (f32, f32),
    ui_scale: f32,
) -> Option<SubLayout> {
    calculate_sub_layout(SubLayoutRequest {
        main_rect,
        active_tool,
        open_dropdown: None,
        screen_width: screen.0,
        screen_height: screen.1,
        ui_scale,
        options: SubLayoutOptions {
            mosaic_mode: MosaicMode::Mosaic,
            smart_redact_enabled: true,
        },
    })
}

fn layout_with_options(
    main_rect: &Rect,
    active_tool: Option<Tool>,
    screen: (f32, f32),
    options: SubLayoutOptions,
) -> Option<SubLayout> {
    calculate_sub_layout(SubLayoutRequest {
        main_rect,
        active_tool,
        open_dropdown: None,
        screen_width: screen.0,
        screen_height: screen.1,
        ui_scale: 1.0,
        options,
    })
}

fn layout_for_dropdown(
    main_rect: &Rect,
    active_tool: Option<Tool>,
    dropdown: Option<SubDropdown>,
    screen: (f32, f32),
) -> Option<SubLayout> {
    calculate_sub_layout(SubLayoutRequest {
        main_rect,
        active_tool,
        open_dropdown: dropdown,
        screen_width: screen.0,
        screen_height: screen.1,
        ui_scale: 1.0,
        options: SubLayoutOptions {
            mosaic_mode: MosaicMode::Mosaic,
            smart_redact_enabled: true,
        },
    })
}

fn main_rect() -> Rect {
    Rect {
        x: 100.0,
        y: 400.0,
        width: 600.0,
        height: 40.0,
    }
}

/// 全屏选区的工具条必须回落到屏幕内，不能停到屏幕底边之外
#[test]
fn test_fullscreen_selection_toolbar_inside_screen() {
    use crate::toolbar::metrics::main_toolbar_width;
    use crate::toolbar::Toolbar;

    let selection = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };
    let width = main_toolbar_width(Toolbar::MAIN_ITEMS, 1.0);
    let layout = calculate_main_layout(MainLayoutRequest {
        selection: &selection,
        screen_width: 1920.0,
        screen_height: 1080.0,
        ui_scale: 1.0,
        width,
    });

    assert!(layout.x >= 8.0);
    assert!(layout.y >= 8.0);
    assert!(layout.x + layout.width <= 1920.0 - 8.0 + 0.01);
    assert!(layout.y + layout.height <= 1080.0 - 8.0 + 0.01);
}

/// 每种工具只暴露相关控件：矩形有圆角、椭圆没有，序号有底板样式
#[test]
fn test_sub_controls_follow_active_tool() {
    let main = main_rect();

    let rect_layout =
        layout_for(&main, Some(Tool::Rect), (1920.0, 1080.0), 1.0).expect("矩形工具应生成二级栏");
    assert!(rect_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::CornerRadius)));
    assert!(rect_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::FillToggle)));

    let ellipse_layout = layout_for(&main, Some(Tool::Ellipse), (1920.0, 1080.0), 1.0)
        .expect("椭圆工具应生成二级栏");
    assert!(!ellipse_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::CornerRadius)));

    let badge_layout =
        layout_for(&main, Some(Tool::Badge), (1920.0, 1080.0), 1.0).expect("序号工具应生成二级栏");
    assert!(badge_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::BadgeStyle)));
    assert!(!badge_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::LineStyle)));

    let mosaic_layout = layout_for(&main, Some(Tool::Mosaic), (1920.0, 1080.0), 1.0)
        .expect("马赛克工具应生成二级栏");
    assert!(mosaic_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::MosaicMode)));
    assert!(mosaic_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::MosaicBrush)));
    assert!(mosaic_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::MosaicRectangle)));
    assert!(mosaic_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::Size)));
    assert!(mosaic_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::EffectStrength)));
    let mosaic_controls = mosaic_layout
        .entries
        .iter()
        .filter_map(|entry| entry.control)
        .collect::<Vec<_>>();
    let position = |control| {
        mosaic_controls
            .iter()
            .position(|entry| *entry == control)
            .unwrap_or(usize::MAX)
    };
    assert!(position(SubControl::Size) < position(SubControl::MosaicMode));
    assert!(position(SubControl::MosaicMode) < position(SubControl::MosaicBrush));
    assert!(position(SubControl::MosaicBrush) < position(SubControl::MosaicRectangle));
    assert!(position(SubControl::MosaicRectangle) < position(SubControl::EffectStrength));

    let smart_redact_layout = layout_with_options(
        &main,
        Some(Tool::Mosaic),
        (1920.0, 1080.0),
        super::SubLayoutOptions {
            mosaic_mode: super::MosaicMode::SmartRedact,
            smart_redact_enabled: true,
        },
    )
    .expect("智能马赛克应生成模式属性栏");
    assert!(smart_redact_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::MosaicMode)));
    for visible in [SubControl::Size, SubControl::EffectStrength] {
        assert!(
            smart_redact_layout
                .entries
                .iter()
                .any(|entry| entry.control == Some(visible)),
            "智能马赛克必须保留 {visible:?} 控件"
        );
    }
    for hidden in [SubControl::MosaicBrush, SubControl::MosaicRectangle] {
        assert!(
            !smart_redact_layout
                .entries
                .iter()
                .any(|entry| entry.control == Some(hidden)),
            "智能马赛克不应显示 {hidden:?}"
        );
    }

    let text_layout =
        layout_for(&main, Some(Tool::Text), (1920.0, 1080.0), 1.0).expect("文字工具应生成二级栏");
    assert!(text_layout
        .entries
        .iter()
        .any(|e| e.control == Some(SubControl::FontSize)));

    for tool in [Tool::Blur, Tool::Spotlight] {
        assert!(
            layout_for(&main, Some(tool), (1920.0, 1080.0), 1.0).is_some(),
            "{tool:?} 必须提供二级属性栏"
        );
    }
}

/// 参考工具栏中的每个工具都必须暴露完整参数，避免后续改动漏掉单个控件。
#[test]
fn test_reference_toolbar_controls_are_complete() {
    let main = main_rect();
    let controls = |tool| {
        layout_for(&main, Some(tool), (1920.0, 1080.0), 1.0)
            .expect("工具应生成二级栏")
            .entries
            .into_iter()
            .filter_map(|entry| entry.control)
            .collect::<Vec<_>>()
    };

    for tool in [Tool::Rect, Tool::Ellipse] {
        let entries = controls(tool);
        for expected in [
            SubControl::ShapeRect,
            SubControl::ShapeEllipse,
            SubControl::FillToggle,
            SubControl::LineStyle,
            SubControl::Size,
        ] {
            assert!(entries.contains(&expected), "{tool:?} 缺少 {expected:?}");
        }
        assert_eq!(
            entries.contains(&SubControl::CornerRadius),
            tool == Tool::Rect,
            "只有矩形需要显示圆角"
        );
    }

    let line = controls(Tool::Line);
    for expected in [
        SubControl::LineMode,
        SubControl::LineStyle,
        SubControl::Size,
        SubControl::ArrowEnabled,
        SubControl::StartArrow,
        SubControl::EndArrow,
    ] {
        assert!(line.contains(&expected), "直线工具缺少 {expected:?}");
    }

    let arrow = controls(Tool::Arrow);
    for expected in [
        SubControl::ArrowStyle,
        SubControl::LineStyle,
        SubControl::Size,
        SubControl::StartArrow,
        SubControl::EndArrow,
    ] {
        assert!(arrow.contains(&expected), "箭头工具缺少 {expected:?}");
    }

    let marker = controls(Tool::Marker);
    for expected in [
        SubControl::MarkerShape,
        SubControl::MarkerBlendMode,
        SubControl::Size,
    ] {
        assert!(marker.contains(&expected), "荧光笔缺少 {expected:?}");
    }

    let text = controls(Tool::Text);
    for expected in [
        SubControl::Bold,
        SubControl::Italic,
        SubControl::TextStroke,
        SubControl::TextBackground,
        SubControl::TextBackgroundOpacity,
        SubControl::TextBackgroundPadding,
        SubControl::TextBackgroundCornerRadius,
        SubControl::FontFamily,
        SubControl::FontSize,
    ] {
        assert!(text.contains(&expected), "文字工具缺少 {expected:?}");
    }

    let badge = controls(Tool::Badge);
    for expected in [
        SubControl::BadgeStyle,
        SubControl::BadgeSequence,
        SubControl::Size,
    ] {
        assert!(badge.contains(&expected), "序号工具缺少 {expected:?}");
    }
    for removed in [
        SubControl::FontFamily,
        SubControl::FontSize,
        SubControl::Bold,
        SubControl::Italic,
        SubControl::StartArrow,
        SubControl::EndArrow,
    ] {
        assert!(
            !badge.contains(&removed),
            "序号工具栏不得包含文字或箭头控件: {removed:?}"
        );
    }

    let mosaic = controls(Tool::Mosaic);
    for expected in [
        SubControl::MosaicBrush,
        SubControl::MosaicRectangle,
        SubControl::MosaicMode,
        SubControl::Size,
        SubControl::EffectStrength,
    ] {
        assert!(mosaic.contains(&expected), "遮挡工具缺少 {expected:?}");
    }

    let spotlight = controls(Tool::Spotlight);
    for expected in [
        SubControl::SpotlightShapeRect,
        SubControl::SpotlightShapeEllipse,
        SubControl::SpotlightIntensity,
        SubControl::SpotlightOutline,
        SubControl::LineStyle,
        SubControl::Size,
    ] {
        assert!(spotlight.contains(&expected), "聚光灯缺少 {expected:?}");
    }

    let magnifier = controls(Tool::Magnifier);
    for expected in [
        SubControl::MagnifierShapeRect,
        SubControl::MagnifierShapeEllipse,
        SubControl::MagnifierScale,
        SubControl::MagnifierSmooth,
        SubControl::MagnifierConnector,
        SubControl::MagnifierShadow,
        SubControl::Size,
    ] {
        assert!(magnifier.contains(&expected), "放大镜缺少 {expected:?}");
    }

    let eraser = controls(Tool::Eraser);
    for expected in [
        SubControl::EraserBrush,
        SubControl::EraserRectangle,
        SubControl::Size,
        SubControl::ClearAnnotations,
    ] {
        assert!(eraser.contains(&expected), "橡皮擦缺少 {expected:?}");
    }
}

/// 所有二级栏只能使用“非空控件组之间的单条分隔线”，禁止首尾或连续分隔线。
#[test]
fn test_sub_bar_separators_only_split_non_empty_groups() {
    use crate::toolbar::items::TOOL_DESCRIPTORS;

    for descriptor in TOOL_DESCRIPTORS.iter().filter(|entry| entry.has_sub_bar) {
        let layout = layout_for(&main_rect(), Some(descriptor.tool), (1920.0, 1080.0), 1.0)
            .unwrap_or_else(|| panic!("{:?} 应生成二级栏", descriptor.tool));
        let entries = &layout.entries;
        assert!(
            entries.first().is_some_and(|entry| entry.control.is_some()),
            "{:?} 二级栏不得以分隔线开头",
            descriptor.tool
        );
        assert!(
            entries.last().is_some_and(|entry| entry.control.is_some()),
            "{:?} 二级栏不得以分隔线结尾",
            descriptor.tool
        );
        assert!(
            entries
                .windows(2)
                .all(|pair| pair[0].control.is_some() || pair[1].control.is_some()),
            "{:?} 二级栏不得出现连续分隔线",
            descriptor.tool
        );
    }
}

/// 屏幕边缘的矩形二级栏必须整体回收，圆角控件不能落到屏幕外。
#[test]
fn test_sub_bar_is_clamped_inside_screen() {
    let main = Rect {
        x: 1260.0,
        y: 300.0,
        width: 600.0,
        height: 40.0,
    };
    let layout =
        layout_for(&main, Some(Tool::Rect), (1366.0, 768.0), 1.0).expect("矩形二级栏应存在");
    assert!(layout.rect.x >= 8.0);
    assert!(layout.rect.x + layout.rect.width <= 1358.0 + 0.01);
    assert!(layout.rect_of(SubControl::CornerRadius).is_some());
}

/// 精简后的序号属性栏仍需完整落在常见 1366 宽屏幕内。
#[test]
fn test_badge_sub_bar_fits_common_screen() {
    let layout = layout_with_options(
        &main_rect(),
        Some(Tool::Badge),
        (1366.0, 768.0),
        super::SubLayoutOptions {
            mosaic_mode: super::MosaicMode::Mosaic,
            smart_redact_enabled: true,
        },
    )
    .expect("序号属性栏应存在");
    assert!(layout.rect.x >= 8.0);
    assert!(
        layout.rect.x + layout.rect.width <= 1358.0,
        "展开属性栏超出屏幕: x={}, width={}",
        layout.rect.x,
        layout.rect.width
    );
}

/// 控件热区必须落在面板内且互不重叠，面板宽度应覆盖到最后一个控件
#[test]
fn test_sub_layout_hit_areas_are_inside_panel_and_disjoint() {
    let layout =
        layout_for(&main_rect(), Some(Tool::Rect), (1920.0, 1080.0), 1.0).expect("应生成二级栏");

    let interactive: Vec<&Rect> = layout
        .entries
        .iter()
        .filter(|e| e.control.is_some())
        .map(|e| &e.rect)
        .collect();
    assert!(interactive.len() >= 9, "矩形工具应至少有 9 个可交互控件");

    for rect in &interactive {
        assert!(
            rect.x >= layout.rect.x && rect.x + rect.width <= layout.rect.x + layout.rect.width,
            "控件超出面板范围: {rect:?} 面板: {:?}",
            layout.rect
        );
    }
    for (i, a) in interactive.iter().enumerate() {
        for b in interactive.iter().skip(i + 1) {
            let overlap = a.x < b.x + b.width
                && b.x < a.x + a.width
                && a.y < b.y + b.height
                && b.y < a.y + a.height;
            assert!(!overlap, "控件热区重叠: {a:?} 与 {b:?}");
        }
    }
}

/// 下拉菜单应锚定在对应控件附近，且行热区完全落在面板内
#[test]
fn test_dropdown_rows_stay_inside_panel() {
    let layout = layout_for_dropdown(
        &main_rect(),
        Some(Tool::Rect),
        Some(SubDropdown::Line),
        (1920.0, 1080.0),
    )
    .expect("应生成二级栏");

    let dropdown = layout.dropdown.as_ref().expect("应生成线型下拉菜单");
    assert_eq!(
        dropdown.rows.len(),
        snapin_annotation::LineStyle::ALL.len(),
        "线型下拉必须覆盖全部线型"
    );
    for row in &dropdown.rows {
        assert!(
            row.x >= dropdown.rect.x
                && row.x + row.width <= dropdown.rect.x + dropdown.rect.width
                && row.y >= dropdown.rect.y
                && row.y + row.height <= dropdown.rect.y + dropdown.rect.height,
            "下拉行超出面板: {row:?}"
        );
    }
    assert!(layout.dropdown_row_hit(dropdown.rows[1].x + 2.0, dropdown.rows[1].y + 2.0) == Some(1));
}

/// 关闭态图标与下拉框必须使用紧凑宽度，避免单图标占据过长热区。
#[test]
fn test_compact_sub_control_widths() {
    let main = main_rect();
    let layout =
        |tool| layout_for(&main, Some(tool), (1920.0, 1080.0), 1.0).expect("工具应生成二级栏");
    let assert_max = |tool: Tool, control: SubControl, max_width: f32| {
        let width = layout(tool)
            .rect_of(control)
            .unwrap_or_else(|| panic!("{tool:?} 缺少 {control:?}"))
            .width;
        assert!(
            width <= max_width,
            "{tool:?} 的 {control:?} 宽度 {width} 超过紧凑上限 {max_width}"
        );
    };

    assert_max(Tool::Rect, SubControl::LineStyle, 48.0);
    assert_max(Tool::Rect, SubControl::CornerRadius, 38.0);
    assert_max(Tool::Rect, SubControl::Size, 36.0);
    for control in [SubControl::BadgeStyle, SubControl::BadgeSequence] {
        assert_max(Tool::Badge, control, 44.0);
    }
    for control in [SubControl::LineMode, SubControl::ArrowEnabled] {
        assert_max(Tool::Line, control, 30.0);
    }
    for control in [SubControl::MarkerShape, SubControl::MarkerBlendMode] {
        assert_max(Tool::Marker, control, 44.0);
    }
    for control in [SubControl::EraserBrush, SubControl::EraserRectangle] {
        assert_max(Tool::Eraser, control, 28.0);
    }
    assert_max(Tool::Mosaic, SubControl::MosaicMode, 48.0);
    assert_max(Tool::Mosaic, SubControl::MosaicBrush, 28.0);
    assert_max(Tool::Mosaic, SubControl::MosaicRectangle, 28.0);
    assert_max(Tool::Spotlight, SubControl::SpotlightIntensity, 42.0);
    assert_max(Tool::Spotlight, SubControl::SpotlightOutline, 30.0);
    assert_max(Tool::Magnifier, SubControl::MagnifierScale, 78.0);
    assert_max(Tool::Magnifier, SubControl::MagnifierSmooth, 30.0);
    assert_max(Tool::Magnifier, SubControl::MagnifierShadow, 30.0);
    for control in [SubControl::TextStroke, SubControl::TextBackground] {
        assert_max(Tool::Text, control, 30.0);
    }
    assert_max(Tool::Text, SubControl::TextBackgroundOpacity, 46.0);
    assert_max(Tool::Text, SubControl::TextBackgroundPadding, 36.0);
    assert_max(Tool::Text, SubControl::TextBackgroundCornerRadius, 38.0);
    for control in [SubControl::FontFamily, SubControl::FontSize] {
        assert_max(Tool::Text, control, 44.0);
    }
}

/// 马赛克画笔与矩形是两个紧邻的图标选择，不应留下大段文字控件宽度。
#[test]
fn test_mosaic_shape_choices_are_adjacent_compact_icons() {
    let layout = layout_for(&main_rect(), Some(Tool::Mosaic), (1920.0, 1080.0), 1.0)
        .expect("马赛克工具应生成二级栏");
    let brush = layout
        .rect_of(SubControl::MosaicBrush)
        .expect("应包含画笔模式");
    let rectangle = layout
        .rect_of(SubControl::MosaicRectangle)
        .expect("应包含矩形模式");
    assert!((brush.x + brush.width - rectangle.x).abs() <= f32::EPSILON);
    assert_eq!(brush.width, rectangle.width);
}
