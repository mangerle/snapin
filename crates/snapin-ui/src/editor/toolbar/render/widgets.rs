//! 工具栏共享绘制原语：面板、分隔线、按钮高亮、圆形与属性预览图元。

use super::super::icons::{add_round_rect, make_paint};
use crate::geometry::Rect;
use snapin_core::annotation::{ArrowStyle, BadgeStyle, LineStyle};
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

/// 绘制通用浅米白微圆角容器背景与微阴影 (参考官方经典现代浅色设计)
pub(super) fn render_panel_background(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
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

/// 绘制按钮悬停与激活背景 (浅灰半透明微胶囊)
pub(super) fn fill_button_highlight(pixmap: &mut PixmapMut, rect: &Rect, color: Color, s: f32) {
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
pub(super) fn render_active_bottom_bar(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
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
pub(super) fn render_divider(pixmap: &mut PixmapMut, x: f32, y: f32, s: f32) {
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

/// 绘制一个实心圆点
pub(super) fn fill_circle(pixmap: &mut PixmapMut, cx: f32, cy: f32, radius: f32, color: Color) {
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
pub(super) fn stroke_circle(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    radius: f32,
    color: Color,
    width: f32,
) {
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
pub(super) fn stroke_sample(
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

/// 绘制圆角半径图标：视觉圆角随当前半径值变化，滚轮调节时可直观看到效果
pub(super) fn render_corner_radius_icon(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    corner_radius: f32,
) {
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
pub(super) fn render_badge_style_icon(
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

/// 绘制迷你箭头预览，三种形态同尺寸对比
pub(super) fn render_arrow_preview(
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
