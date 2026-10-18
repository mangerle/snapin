//! 工具栏与二级属性栏的纯 tiny-skia 矢量光栅化绘制。

use super::icons::{add_round_rect, make_paint, render_item_icon};
use super::items::{preset_colors, StrokeWidthLevel, ToolbarItem};
use super::Toolbar;
use crate::snipper::selection::Rect;
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
    let btn_size = 32.0 * s;
    let padding = 6.0 * s;
    let spacing = 4.0 * s;
    let mut cur_x = toolbar.main_rect.x + padding;

    for (idx, &item) in Toolbar::MAIN_ITEMS.iter().enumerate() {
        if idx == 7 || idx == 9 {
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
            fill_button_highlight(pixmap, &btn_rect, Color::from_rgba8(0, 0, 0, 28), s);
            render_active_bottom_bar(pixmap, &btn_rect, s);
        } else if is_hovered {
            fill_button_highlight(pixmap, &btn_rect, Color::from_rgba8(0, 0, 0, 16), s);
            if item == ToolbarItem::ActionSave {
                render_active_bottom_bar(pixmap, &btn_rect, s);
            }
        }

        render_item_icon(pixmap, item, &btn_rect, s, toolbar.sub_options.color);
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
                make_paint(Color::from_rgba8(37, 99, 235, 255))
            } else {
                make_paint(Color::from_rgba8(165, 170, 182, 255))
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

            // 色块浅灰微描边
            let border_paint = make_paint(Color::from_rgba8(0, 0, 0, 32));
            let border_stroke = Stroke {
                width: 1.0 * s,
                ..Default::default()
            };
            pixmap.stroke_path(
                &path,
                &border_paint,
                &border_stroke,
                Transform::identity(),
                None,
            );

            // 选中色块外围包裹深色指示圆环
            if is_selected {
                let mut ring_pb = PathBuilder::new();
                ring_pb.push_circle(cur_x + 8.0 * s, center_y, 10.5 * s);
                if let Some(ring_path) = ring_pb.finish() {
                    let ring_paint = make_paint(Color::from_rgba8(45, 48, 56, 220));
                    let ring_stroke = Stroke {
                        width: 1.8 * s,
                        ..Default::default()
                    };
                    pixmap.stroke_path(
                        &ring_path,
                        &ring_paint,
                        &ring_stroke,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
        cur_x += 26.0 * s;
    }
}
