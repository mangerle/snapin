//! 工具栏与二级属性栏的纯 tiny-skia 矢量光栅化绘制。

use super::items::{preset_colors, StrokeWidthLevel, ToolbarItem};
use super::Toolbar;
use crate::snipper::selection::Rect;
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Rect as SkRect, Stroke, Transform};

/// 创建开启抗锯齿的基础画笔
fn make_paint(color: Color) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(color);
    paint
}

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

/// 绘制通用暗色圆角容器背景与 1px 微亮边框
fn render_panel_background(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
    if let Some(sk_rect) = SkRect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
        // 深灰微透底色 #1E1E2E，92% 不透明
        let paint = make_paint(Color::from_rgba8(30, 30, 46, 235));
        let path = PathBuilder::from_rect(sk_rect);
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        // 1px 半透明白色外描边 (随 HiDPI 缩放)
        let stroke_paint = make_paint(Color::from_rgba8(255, 255, 255, 38));
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
    let btn_size = 32.0 * s;
    let padding = 6.0 * s;
    let spacing = 4.0 * s;
    let mut cur_x = toolbar.main_rect.x + padding;

    for (idx, &item) in Toolbar::MAIN_ITEMS.iter().enumerate() {
        if idx == 8 || idx == 10 {
            render_divider(pixmap, cur_x + 3.0 * s, toolbar.main_rect.y + padding, s);
            cur_x += 8.0 * s;
        }

        let btn_rect = Rect {
            x: cur_x,
            y: toolbar.main_rect.y + padding,
            width: btn_size,
            height: btn_size,
        };

        let is_active = toolbar.active_tool == Some(item);
        let is_hovered = toolbar.hovered_item == Some(item);

        if is_active {
            fill_button_highlight(pixmap, &btn_rect, Color::from_rgba8(59, 130, 246, 200), s);
        } else if is_hovered {
            fill_button_highlight(pixmap, &btn_rect, Color::from_rgba8(255, 255, 255, 40), s);
        }

        render_item_icon(pixmap, item, &btn_rect, s);
        cur_x += btn_size + spacing;
    }
}

/// 绘制按钮悬停与激活背景
fn fill_button_highlight(pixmap: &mut PixmapMut, rect: &Rect, color: Color, s: f32) {
    if let Some(sk_rect) = SkRect::from_xywh(
        rect.x + 2.0 * s,
        rect.y + 2.0 * s,
        rect.width - 4.0 * s,
        rect.height - 4.0 * s,
    ) {
        let paint = make_paint(color);
        let path = PathBuilder::from_rect(sk_rect);
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制垂直细分隔线
fn render_divider(pixmap: &mut PixmapMut, x: f32, y: f32, s: f32) {
    let mut pb = PathBuilder::new();
    pb.move_to(x, y + 4.0 * s);
    pb.line_to(x, y + 28.0 * s);
    if let Some(path) = pb.finish() {
        let paint = make_paint(Color::from_rgba8(255, 255, 255, 45));
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制各项的具体矢量图标 (自适应 HiDPI 缩放)
fn render_item_icon(pixmap: &mut PixmapMut, item: ToolbarItem, rect: &Rect, s: f32) {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let paint = make_paint(Color::from_rgba8(240, 240, 245, 255));

    let stroke_2px = Stroke {
        width: 2.0 * s,
        ..Default::default()
    };

    match item {
        ToolbarItem::ToolRect => {
            if let Some(r) = SkRect::from_xywh(cx - 7.0 * s, cy - 7.0 * s, 14.0 * s, 14.0 * s) {
                let path = PathBuilder::from_rect(r);
                pixmap.stroke_path(&path, &paint, &stroke_2px, Transform::identity(), None);
            }
        }
        ToolbarItem::ToolEllipse => {
            let mut pb = PathBuilder::new();
            pb.push_circle(cx, cy, 7.0 * s);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_2px, Transform::identity(), None);
            }
        }
        ToolbarItem::ToolArrow => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 7.0 * s, cy + 7.0 * s);
            pb.line_to(cx + 6.0 * s, cy - 6.0 * s);
            pb.line_to(cx + 1.0 * s, cy - 6.0 * s);
            pb.move_to(cx + 6.0 * s, cy - 6.0 * s);
            pb.line_to(cx + 6.0 * s, cy - 1.0 * s);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_2px, Transform::identity(), None);
            }
        }
        ToolbarItem::ToolPen => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 6.0 * s, cy + 6.0 * s);
            pb.line_to(cx + 4.0 * s, cy - 4.0 * s);
            pb.line_to(cx + 7.0 * s, cy - 1.0 * s);
            pb.line_to(cx - 3.0 * s, cy + 7.0 * s);
            pb.close();
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_2px, Transform::identity(), None);
            }
        }
        ToolbarItem::ToolMarker => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 5.0 * s, cy + 6.0 * s);
            pb.line_to(cx + 5.0 * s, cy - 4.0 * s);
            if let Some(path) = pb.finish() {
                let marker_stroke = Stroke {
                    width: 5.0 * s,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &paint, &marker_stroke, Transform::identity(), None);
            }
        }
        ToolbarItem::ToolMosaic => {
            for i in 0..2 {
                for j in 0..2 {
                    let rx = cx - 6.0 * s + (i as f32) * 6.0 * s;
                    let ry = cy - 6.0 * s + (j as f32) * 6.0 * s;
                    if let Some(r) = SkRect::from_xywh(rx, ry, 5.0 * s, 5.0 * s) {
                        let path = PathBuilder::from_rect(r);
                        if (i + j) % 2 == 0 {
                            pixmap.fill_path(
                                &path,
                                &paint,
                                tiny_skia::FillRule::Winding,
                                Transform::identity(),
                                None,
                            );
                        } else {
                            pixmap.stroke_path(
                                &path,
                                &paint,
                                &stroke_2px,
                                Transform::identity(),
                                None,
                            );
                        }
                    }
                }
            }
        }
        ToolbarItem::ToolText => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 6.0 * s, cy - 6.0 * s);
            pb.line_to(cx + 6.0 * s, cy - 6.0 * s);
            pb.move_to(cx, cy - 6.0 * s);
            pb.line_to(cx, cy + 7.0 * s);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_2px, Transform::identity(), None);
            }
        }
        ToolbarItem::ToolBadge => {
            let mut pb = PathBuilder::new();
            pb.push_circle(cx, cy, 7.5 * s);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_2px, Transform::identity(), None);
            }
            let mut digit_pb = PathBuilder::new();
            digit_pb.move_to(cx - 2.0 * s, cy - 2.5 * s);
            digit_pb.line_to(cx, cy - 4.5 * s);
            digit_pb.line_to(cx, cy + 4.5 * s);
            if let Some(dpath) = digit_pb.finish() {
                let digit_stroke = Stroke {
                    width: 1.6 * s,
                    ..Default::default()
                };
                pixmap.stroke_path(&dpath, &paint, &digit_stroke, Transform::identity(), None);
            }
        }
        ToolbarItem::ActionUndo => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 6.0 * s, cy);
            pb.line_to(cx - 2.0 * s, cy - 4.0 * s);
            pb.move_to(cx - 6.0 * s, cy);
            pb.line_to(cx - 2.0 * s, cy + 4.0 * s);
            pb.move_to(cx - 6.0 * s, cy);
            pb.line_to(cx + 5.0 * s, cy);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_2px, Transform::identity(), None);
            }
        }
        ToolbarItem::ActionRedo => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx + 6.0 * s, cy);
            pb.line_to(cx + 2.0 * s, cy - 4.0 * s);
            pb.move_to(cx + 6.0 * s, cy);
            pb.line_to(cx + 2.0 * s, cy + 4.0 * s);
            pb.move_to(cx + 6.0 * s, cy);
            pb.line_to(cx - 5.0 * s, cy);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_2px, Transform::identity(), None);
            }
        }
        ToolbarItem::ActionPin => {
            let mut pb = PathBuilder::new();
            pb.push_circle(cx, cy - 3.0 * s, 4.0 * s);
            pb.move_to(cx, cy + 1.0 * s);
            pb.line_to(cx, cy + 8.0 * s);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_2px, Transform::identity(), None);
            }
        }
        ToolbarItem::ActionSave => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx, cy - 6.0 * s);
            pb.line_to(cx, cy + 3.0 * s);
            pb.line_to(cx - 4.0 * s, cy - 1.0 * s);
            pb.move_to(cx, cy + 3.0 * s);
            pb.line_to(cx + 4.0 * s, cy - 1.0 * s);
            pb.move_to(cx - 6.0 * s, cy + 7.0 * s);
            pb.line_to(cx + 6.0 * s, cy + 7.0 * s);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_2px, Transform::identity(), None);
            }
        }
        ToolbarItem::ActionCopy => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 6.0 * s, cy);
            pb.line_to(cx - 2.0 * s, cy + 5.0 * s);
            pb.line_to(cx + 7.0 * s, cy - 5.0 * s);
            if let Some(path) = pb.finish() {
                let check_paint = make_paint(Color::from_rgba8(34, 197, 94, 255));
                pixmap.stroke_path(
                    &path,
                    &check_paint,
                    &stroke_2px,
                    Transform::identity(),
                    None,
                );
            }
        }
        ToolbarItem::ActionClose => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 5.0 * s, cy - 5.0 * s);
            pb.line_to(cx + 5.0 * s, cy + 5.0 * s);
            pb.move_to(cx + 5.0 * s, cy - 5.0 * s);
            pb.line_to(cx - 5.0 * s, cy + 5.0 * s);
            if let Some(path) = pb.finish() {
                let close_paint = make_paint(Color::from_rgba8(239, 68, 68, 255));
                pixmap.stroke_path(
                    &path,
                    &close_paint,
                    &stroke_2px,
                    Transform::identity(),
                    None,
                );
            }
        }
    }
}

/// 绘制二级属性栏内容 (3 档粗细 + 6 个预设色块，自适应 HiDPI 缩放)
fn render_sub_options(toolbar: &Toolbar, pixmap: &mut PixmapMut, rect: &Rect) {
    let s = toolbar.ui_scale;
    let padding = 12.0 * s;
    let mut cur_x = rect.x + padding;
    let center_y = rect.y + rect.height / 2.0;

    for level in [
        StrokeWidthLevel::Thin,
        StrokeWidthLevel::Medium,
        StrokeWidthLevel::Thick,
    ] {
        let radius = match level {
            StrokeWidthLevel::Thin => 2.5 * s,
            StrokeWidthLevel::Medium => 4.5 * s,
            StrokeWidthLevel::Thick => 7.0 * s,
        };

        let is_selected = toolbar.sub_options.stroke_width == level;
        let mut pb = PathBuilder::new();
        pb.push_circle(cur_x + 6.0 * s, center_y, radius);
        if let Some(path) = pb.finish() {
            let p = if is_selected {
                make_paint(Color::from_rgba8(59, 130, 246, 255))
            } else {
                make_paint(Color::from_rgba8(200, 200, 210, 200))
            };
            pixmap.fill_path(
                &path,
                &p,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        cur_x += 24.0 * s;
    }

    render_divider(pixmap, cur_x + 6.0 * s, rect.y + 4.0 * s, s);
    cur_x += 20.0 * s;

    for color in preset_colors() {
        let is_selected = toolbar.sub_options.color == color;
        let mut pb = PathBuilder::new();
        pb.push_circle(cur_x + 8.0 * s, center_y, 8.0 * s);
        if let Some(path) = pb.finish() {
            let p = make_paint(color);
            pixmap.fill_path(
                &path,
                &p,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );

            if is_selected {
                let ring_paint = make_paint(Color::from_rgba8(255, 255, 255, 255));
                let ring_stroke = Stroke {
                    width: 2.0 * s,
                    ..Default::default()
                };
                pixmap.stroke_path(
                    &path,
                    &ring_paint,
                    &ring_stroke,
                    Transform::identity(),
                    None,
                );
            }
        }
        cur_x += 26.0 * s;
    }
}
