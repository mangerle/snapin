//! 工具栏与二级属性栏的纯 tiny-skia 矢量光栅化绘制。

use super::icons::{add_round_rect, make_paint, render_item_icon};
use super::items::{preset_colors, StrokeWidthLevel, ToolbarItem, CORNER_RADIUS_PRESETS};
use super::layout::{DropdownLayout, SubControl, SubDropdown};
use super::metrics::{button_size, divider_width, has_separator_before, padding, spacing};
use super::Toolbar;
use crate::snipper::selection::Rect;
use snapin_core::annotation::{ArrowStyle, BadgeStyle, LineStyle};
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

/// 渲染整个工具栏界面 (包含主工具栏与二级属性栏)
pub fn render_toolbar(toolbar: &Toolbar, pixmap: &mut PixmapMut) {
    if toolbar.main_rect.width <= 0.0 || toolbar.main_rect.height <= 0.0 {
        return;
    }
    let s = toolbar.ui_scale;

    // 1. 绘制主工具栏面板背景
    render_panel_background(pixmap, &toolbar.main_rect, s);

    // 2. 绘制各个主按钮与矢量图标
    render_main_buttons(toolbar, pixmap);

    // 3. 绘制二级属性栏 (若激活)
    if let Some(sub_rect) = toolbar.sub_rect {
        render_panel_background(pixmap, &sub_rect, s);
        render_sub_options(toolbar, pixmap, &sub_rect);
    }
}

/// 绘制通用浅米白微圆角容器背景与微阴影 (参考官方经典现代浅色设计)
fn render_panel_background(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
    // 1. 底部浅淡微投影
    let mut pb_shadow = PathBuilder::new();
    add_round_rect(
        &mut pb_shadow,
        rect.x,
        rect.y + 1.5 * s,
        rect.width,
        rect.height,
        6.0 * s,
    );
    if let Some(path) = pb_shadow.finish() {
        let shadow_paint = make_paint(Color::from_rgba8(0, 0, 0, 16));
        pixmap.fill_path(
            &path,
            &shadow_paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    // 2. 浅灰米白底色卡片主体
    let mut pb = PathBuilder::new();
    add_round_rect(&mut pb, rect.x, rect.y, rect.width, rect.height, 6.0 * s);
    if let Some(path) = pb.finish() {
        let paint = make_paint(Color::from_rgba8(243, 244, 246, 252));
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        let stroke_paint = make_paint(Color::from_rgba8(0, 0, 0, 24));
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制主工具栏的各个按钮项
fn render_main_buttons(toolbar: &Toolbar, pixmap: &mut PixmapMut) {
    let s = toolbar.ui_scale;
    let btn_size = button_size(s);
    let padding = padding(s);
    let spacing = spacing(s);
    let mut cur_x = toolbar.main_rect.x + padding;

    for (idx, &item) in Toolbar::MAIN_ITEMS.iter().enumerate() {
        if has_separator_before(idx) {
            render_divider(pixmap, cur_x + 3.0 * s, toolbar.main_rect.y + padding, s);
            cur_x += divider_width(s);
        }

        let btn_rect = Rect {
            x: cur_x,
            y: toolbar.main_rect.y + padding,
            width: btn_size,
            height: btn_size,
        };

        let is_geometric_slot = item == ToolbarItem::ToolRect;
        let is_active = if is_geometric_slot {
            toolbar.active_tool == Some(ToolbarItem::ToolRect)
                || toolbar.active_tool == Some(ToolbarItem::ToolEllipse)
        } else {
            toolbar.active_tool == Some(item)
        };
        let is_hovered = toolbar.hovered_item == Some(item);

        if is_active {
            fill_button_highlight(pixmap, &btn_rect, Color::from_rgba8(0, 0, 0, 28), s);
            render_active_bottom_bar(pixmap, &btn_rect, s);
        } else if is_hovered {
            fill_button_highlight(pixmap, &btn_rect, Color::from_rgba8(0, 0, 0, 16), s);
            if item == ToolbarItem::ActionSave {
                render_active_bottom_bar(pixmap, &btn_rect, s);
            }
        }

        let display_item =
            if is_geometric_slot && toolbar.active_tool == Some(ToolbarItem::ToolEllipse) {
                ToolbarItem::ToolEllipse
            } else {
                item
            };

        render_item_icon(
            pixmap,
            display_item,
            &btn_rect,
            s,
            toolbar.sub_options.color,
        );
        cur_x += btn_size + spacing;
    }
}

/// 绘制按钮悬停与激活背景 (浅灰半透明微胶囊)
fn fill_button_highlight(pixmap: &mut PixmapMut, rect: &Rect, color: Color, s: f32) {
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        rect.x + 2.0 * s,
        rect.y + 2.0 * s,
        rect.width - 4.0 * s,
        rect.height - 4.0 * s,
        5.0 * s,
    );
    if let Some(path) = pb.finish() {
        let paint = make_paint(color);
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制激活或保存悬停时底部的指示细蓝条
fn render_active_bottom_bar(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
    let mut pb = PathBuilder::new();
    pb.move_to(rect.x + 6.0 * s, rect.y + rect.height - 2.0 * s);
    pb.line_to(
        rect.x + rect.width - 6.0 * s,
        rect.y + rect.height - 2.0 * s,
    );
    if let Some(path) = pb.finish() {
        let paint = make_paint(Color::from_rgba8(37, 99, 235, 255));
        let stroke = Stroke {
            width: 2.2 * s,
            line_cap: tiny_skia::LineCap::Round,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制垂直细分隔线
fn render_divider(pixmap: &mut PixmapMut, x: f32, y: f32, s: f32) {
    let mut pb = PathBuilder::new();
    pb.move_to(x, y + 5.0 * s);
    pb.line_to(x, y + 27.0 * s);
    if let Some(path) = pb.finish() {
        let paint = make_paint(Color::from_rgba8(0, 0, 0, 26));
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制二级属性栏内容 (形状切换 + 3 档粗细 + 6 个预设色块，自适应 HiDPI 缩放)
fn render_sub_options(toolbar: &Toolbar, pixmap: &mut PixmapMut, rect: &Rect) {
    let Some(layout) = toolbar.sub_layout() else {
        return;
    };
    let s = toolbar.ui_scale;
    let center_y = rect.y + rect.height / 2.0;
    let color = toolbar.sub_options.color;
    let highlight = Color::from_rgba8(37, 99, 235, 45);
    let presets = preset_colors();

    for entry in &layout.entries {
        match entry.control {
            None => render_divider(pixmap, entry.rect.x + 8.0 * s, rect.y + 8.0 * s, s),
            Some(SubControl::ShapeRect) => {
                let active = toolbar.active_tool == Some(ToolbarItem::ToolRect);
                if active {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                render_sub_shape_rect(pixmap, entry.rect.x + 12.0 * s, center_y, s, active);
            }
            Some(SubControl::ShapeEllipse) => {
                let active = toolbar.active_tool == Some(ToolbarItem::ToolEllipse);
                if active {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                render_sub_shape_ellipse(pixmap, entry.rect.x + 12.0 * s, center_y, s, active);
            }
            Some(SubControl::Stroke(level)) => {
                let radius = match level {
                    StrokeWidthLevel::Thin => 2.5 * s,
                    StrokeWidthLevel::Medium => 4.5 * s,
                    StrokeWidthLevel::Thick => 7.0 * s,
                };
                let selected = toolbar.sub_options.stroke_width == level;
                let paint = if selected {
                    Color::from_rgba8(37, 99, 235, 255)
                } else {
                    Color::from_rgba8(165, 170, 182, 255)
                };
                fill_circle(pixmap, entry.rect.x + 8.0 * s, center_y, radius, paint);
            }
            Some(SubControl::LineStyle) => {
                let open = toolbar.open_dropdown == Some(SubDropdown::Line);
                if open {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                stroke_sample(
                    pixmap,
                    entry.rect.x + 8.0 * s,
                    entry.rect.x + entry.rect.width - 14.0 * s,
                    center_y,
                    2.0 * s,
                    toolbar.sub_options.line_style,
                    Color::from_rgba8(45, 48, 56, 255),
                );
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 8.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::FillToggle) => {
                render_fill_toggle(
                    pixmap,
                    entry.rect.x + 12.0 * s,
                    center_y,
                    s,
                    toolbar.sub_options.filled,
                    color,
                );
            }
            Some(SubControl::CornerRadius) => {
                render_corner_radius_icon(
                    pixmap,
                    entry.rect.x + 12.0 * s,
                    center_y,
                    s,
                    toolbar.sub_options.corner_radius,
                );
            }
            Some(SubControl::BadgeStyle) => {
                let open = toolbar.open_dropdown == Some(SubDropdown::Badge);
                if open {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                render_badge_style_icon(
                    pixmap,
                    entry.rect.x + 12.0 * s,
                    center_y,
                    s,
                    toolbar.sub_options.badge_style,
                    color,
                );
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 8.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::ArrowStyle) => {
                let open = toolbar.open_dropdown == Some(SubDropdown::Arrow);
                if open {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                render_arrow_preview(
                    pixmap,
                    entry.rect.x + 8.0 * s,
                    entry.rect.x + entry.rect.width - 14.0 * s,
                    center_y,
                    s,
                    toolbar.sub_options.arrow_style,
                    Color::from_rgba8(45, 48, 56, 255),
                );
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 8.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::Color(index)) => {
                let Some(preset) = presets.get(index) else {
                    continue;
                };
                let is_selected = toolbar.sub_options.color == *preset;
                let cx = entry.rect.x + 8.0 * s;
                fill_circle(pixmap, cx, center_y, 8.0 * s, *preset);
                stroke_circle(
                    pixmap,
                    cx,
                    center_y,
                    8.0 * s,
                    Color::from_rgba8(0, 0, 0, 32),
                    1.0 * s,
                );
                if is_selected {
                    stroke_circle(
                        pixmap,
                        cx,
                        center_y,
                        10.5 * s,
                        Color::from_rgba8(45, 48, 56, 220),
                        1.8 * s,
                    );
                }
            }
        }
    }

    if let Some(dropdown) = &layout.dropdown {
        render_dropdown(pixmap, dropdown, toolbar, s);
    }
}

/// 绘制一个实心圆点
fn fill_circle(pixmap: &mut PixmapMut, cx: f32, cy: f32, radius: f32, color: Color) {
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, radius);
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制一个圆形描边
fn stroke_circle(pixmap: &mut PixmapMut, cx: f32, cy: f32, radius: f32, color: Color, width: f32) {
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, radius);
    if let Some(path) = pb.finish() {
        let stroke = Stroke {
            width,
            ..Default::default()
        };
        pixmap.stroke_path(
            &path,
            &make_paint(color),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

/// 按线型绘制一条水平示意线
fn stroke_sample(
    pixmap: &mut PixmapMut,
    x0: f32,
    x1: f32,
    y: f32,
    width: f32,
    style: LineStyle,
    color: Color,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(x0, y);
    pb.line_to(x1, y);
    if let Some(path) = pb.finish() {
        style.stroke_path(pixmap, &path, &make_paint(color), width);
    }
}

/// 绘制下拉按钮右侧的展开箭头
fn render_caret(pixmap: &mut PixmapMut, cx: f32, cy: f32, s: f32) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 2.5 * s, cy - 1.5 * s);
    pb.line_to(cx, cy + 1.5 * s);
    pb.line_to(cx + 2.5 * s, cy - 1.5 * s);
    if let Some(path) = pb.finish() {
        let stroke = Stroke {
            width: 1.4 * s,
            ..Default::default()
        };
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(100, 106, 120, 255)),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制“仅边框 / 边框+填充”开关图标
fn render_fill_toggle(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    filled: bool,
    color: Color,
) {
    let half = 7.0 * s;
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        cx - half,
        cy - half,
        half * 2.0,
        half * 2.0,
        2.0 * s,
    );
    if let Some(path) = pb.finish() {
        if filled {
            pixmap.fill_path(
                &path,
                &make_paint(color),
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        } else {
            let stroke = Stroke {
                width: 1.8 * s,
                ..Default::default()
            };
            pixmap.stroke_path(
                &path,
                &make_paint(Color::from_rgba8(90, 96, 110, 255)),
                &stroke,
                Transform::identity(),
                None,
            );
        }
    }
}

/// 绘制圆角半径图标：视觉圆角随当前半径值变化，滚轮调节时可直观看到效果
fn render_corner_radius_icon(pixmap: &mut PixmapMut, cx: f32, cy: f32, s: f32, corner_radius: f32) {
    let half = 7.0 * s;
    let visual_radius = (1.0 + corner_radius / 30.0 * 5.0) * s;
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        cx - half,
        cy - half,
        half * 2.0,
        half * 2.0,
        visual_radius,
    );
    if let Some(path) = pb.finish() {
        let stroke = Stroke {
            width: 1.8 * s,
            ..Default::default()
        };
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(90, 96, 110, 255)),
            &stroke,
            Transform::identity(),
            None,
        );
        // 左上角高亮弧，暗示圆角可调
        let mut arc = PathBuilder::new();
        arc.move_to(cx - half, cy - half + visual_radius);
        arc.quad_to(cx - half, cy - half, cx - half + visual_radius, cy - half);
        if let Some(arc_path) = arc.finish() {
            let arc_stroke = Stroke {
                width: 2.2 * s,
                ..Default::default()
            };
            pixmap.stroke_path(
                &arc_path,
                &make_paint(Color::from_rgba8(37, 99, 235, 255)),
                &arc_stroke,
                Transform::identity(),
                None,
            );
        }
    }
}

/// 绘制序号底板样式图标（实心圆 / 空心环）
fn render_badge_style_icon(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    style: BadgeStyle,
    color: Color,
) {
    match style {
        BadgeStyle::Filled => fill_circle(pixmap, cx, cy, 7.0 * s, color),
        BadgeStyle::Outline => stroke_circle(pixmap, cx, cy, 6.5 * s, color, 2.0 * s),
    }
}

/// 绘制展开的下拉菜单面板与选项预览
fn render_dropdown(pixmap: &mut PixmapMut, dropdown: &DropdownLayout, toolbar: &Toolbar, s: f32) {
    let rect = dropdown.rect;

    // 柔影与白色圆角面板
    let mut shadow_pb = PathBuilder::new();
    add_round_rect(
        &mut shadow_pb,
        rect.x + 1.0 * s,
        rect.y + 2.0 * s,
        rect.width,
        rect.height,
        8.0 * s,
    );
    if let Some(path) = shadow_pb.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(0, 0, 0, 36)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let mut pb = PathBuilder::new();
    add_round_rect(&mut pb, rect.x, rect.y, rect.width, rect.height, 8.0 * s);
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(255, 255, 255, 255)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
        let border = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(218, 224, 233, 255)),
            &border,
            Transform::identity(),
            None,
        );
    }

    let ink = Color::from_rgba8(45, 48, 56, 255);
    for (index, row) in dropdown.rows.iter().enumerate() {
        let (is_current, preview) = match dropdown.kind {
            SubDropdown::Line => {
                let Some(style) = LineStyle::ALL.get(index) else {
                    continue;
                };
                (
                    toolbar.sub_options.line_style == *style,
                    DropdownPreview::Line(*style),
                )
            }
            SubDropdown::Badge => {
                let style = if index == 0 {
                    BadgeStyle::Filled
                } else {
                    BadgeStyle::Outline
                };
                (
                    toolbar.sub_options.badge_style == style,
                    DropdownPreview::Badge(style),
                )
            }
            SubDropdown::Arrow => {
                let Some(style) = ArrowStyle::ALL.get(index) else {
                    continue;
                };
                (
                    toolbar.sub_options.arrow_style == *style,
                    DropdownPreview::Arrow(*style),
                )
            }
            SubDropdown::Corner => {
                let Some(radius) = CORNER_RADIUS_PRESETS.get(index) else {
                    continue;
                };
                (
                    (toolbar.sub_options.corner_radius - *radius).abs() < 0.5,
                    DropdownPreview::Corner(*radius),
                )
            }
        };

        if is_current {
            let mut row_pb = PathBuilder::new();
            add_round_rect(&mut row_pb, row.x, row.y, row.width, row.height, 5.0 * s);
            if let Some(path) = row_pb.finish() {
                pixmap.fill_path(
                    &path,
                    &make_paint(Color::from_rgba8(37, 99, 235, 28)),
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        } else if toolbar.dropdown_hover == Some(index) {
            let mut row_pb = PathBuilder::new();
            add_round_rect(&mut row_pb, row.x, row.y, row.width, row.height, 5.0 * s);
            if let Some(path) = row_pb.finish() {
                pixmap.fill_path(
                    &path,
                    &make_paint(Color::from_rgba8(148, 163, 184, 34)),
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }

        let center_y = row.y + row.height / 2.0;
        match preview {
            DropdownPreview::Line(style) => stroke_sample(
                pixmap,
                row.x + 10.0 * s,
                row.x + row.width - 24.0 * s,
                center_y,
                2.0 * s,
                style,
                ink,
            ),
            DropdownPreview::Badge(style) => {
                render_badge_style_icon(pixmap, row.x + 18.0 * s, center_y, s, style, ink)
            }
            DropdownPreview::Corner(radius) => {
                render_corner_radius_icon(pixmap, row.x + 18.0 * s, center_y, s, radius)
            }
            DropdownPreview::Arrow(style) => render_arrow_preview(
                pixmap,
                row.x + 10.0 * s,
                row.x + row.width - 24.0 * s,
                center_y,
                s,
                style,
                ink,
            ),
        }

        if is_current {
            fill_circle(
                pixmap,
                row.x + row.width - 12.0 * s,
                center_y,
                3.0 * s,
                Color::from_rgba8(37, 99, 235, 255),
            );
        }
    }
}

/// 下拉行的预览类型
enum DropdownPreview {
    Line(LineStyle),
    Badge(BadgeStyle),
    Arrow(ArrowStyle),
    Corner(f32),
}

/// 绘制迷你箭头预览，三种形态同尺寸对比
fn render_arrow_preview(
    pixmap: &mut PixmapMut,
    x0: f32,
    x1: f32,
    cy: f32,
    s: f32,
    style: ArrowStyle,
    color: Color,
) {
    let head_len = 8.0 * s;
    let head_half = 4.0 * s;
    let shaft_half = 1.2 * s;
    let base_x = x1 - head_len;

    match style.shaft_line_style() {
        Some(line_style) => {
            stroke_sample(pixmap, x0, base_x, cy, 2.0 * s, line_style, color);
        }
        None => {
            let tail_half = shaft_half * 0.32;
            let mut pb = PathBuilder::new();
            pb.move_to(x0, cy - tail_half);
            pb.line_to(base_x, cy - shaft_half);
            pb.line_to(base_x, cy - head_half);
            pb.line_to(x1, cy);
            pb.line_to(base_x, cy + head_half);
            pb.line_to(base_x, cy + shaft_half);
            pb.line_to(x0, cy + tail_half);
            pb.close();
            if let Some(path) = pb.finish() {
                pixmap.fill_path(
                    &path,
                    &make_paint(color),
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
            return;
        }
    }

    let mut head = PathBuilder::new();
    head.move_to(x1, cy);
    head.line_to(base_x, cy - head_half);
    head.line_to(base_x, cy + head_half);
    head.close();
    if let Some(path) = head.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制二级属性栏中的微型矩形形状图标
fn render_sub_shape_rect(pixmap: &mut PixmapMut, cx: f32, cy: f32, s: f32, is_active: bool) {
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        cx - 6.5 * s,
        cy - 5.5 * s,
        13.0 * s,
        11.0 * s,
        1.5 * s,
    );
    if let Some(path) = pb.finish() {
        let color = if is_active {
            Color::from_rgba8(37, 99, 235, 255)
        } else {
            Color::from_rgba8(75, 85, 99, 255)
        };
        let paint = make_paint(color);
        let stroke = Stroke {
            width: 1.6 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制二级属性栏中的微型椭圆形状图标
fn render_sub_shape_ellipse(pixmap: &mut PixmapMut, cx: f32, cy: f32, s: f32, is_active: bool) {
    let mut pb = PathBuilder::new();
    let a = 6.5 * s;
    let b = 5.5 * s;
    let ox = a * 0.552_284_8_f32;
    let oy = b * 0.552_284_8_f32;
    pb.move_to(cx - a, cy);
    pb.cubic_to(cx - a, cy - oy, cx - ox, cy - b, cx, cy - b);
    pb.cubic_to(cx + ox, cy - b, cx + a, cy - oy, cx + a, cy);
    pb.cubic_to(cx + a, cy + oy, cx + ox, cy + b, cx, cy + b);
    pb.cubic_to(cx - ox, cy + b, cx - a, cy + oy, cx - a, cy);
    pb.close();

    if let Some(path) = pb.finish() {
        let color = if is_active {
            Color::from_rgba8(37, 99, 235, 255)
        } else {
            Color::from_rgba8(75, 85, 99, 255)
        };
        let paint = make_paint(color);
        let stroke = Stroke {
            width: 1.6 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}
