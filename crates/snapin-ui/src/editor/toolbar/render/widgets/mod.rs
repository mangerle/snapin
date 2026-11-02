//! 工具栏共享绘制原语：面板、分隔线、按钮高亮、圆形与属性预览图元。

mod arrow;

pub(super) use arrow::{render_arrow_head_preview, render_arrow_preview, ArrowHeadPreviewSpec};

use super::super::icons::{add_round_rect, make_paint, RoundRect};
use crate::geometry::Rect;
use snapin_annotation::{BadgeStyle, LineStyle, MagnifierConnector};
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 绘制通用浅米白微圆角容器背景与微阴影 (参考官方经典现代浅色设计)
pub(super) fn render_panel_background(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
    // 1. 底部浅淡微投影
    let mut pb_shadow = PathBuilder::new();
    add_round_rect(
        &mut pb_shadow,
        RoundRect {
            x: rect.x,
            y: rect.y + 1.5 * s,
            width: rect.width,
            height: rect.height,
            radius: 6.0 * s,
        },
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
    add_round_rect(
        &mut pb,
        RoundRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            radius: 6.0 * s,
        },
    );
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
        RoundRect {
            x: rect.x + 2.0 * s,
            y: rect.y + 2.0 * s,
            width: rect.width - 4.0 * s,
            height: rect.height - 4.0 * s,
            radius: 5.0 * s,
        },
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
    let visual_radius = (1.0 + corner_radius / 90.0 * 5.0) * s;
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: cx - half,
            y: cy - half,
            width: half * 2.0,
            height: half * 2.0,
            radius: visual_radius,
        },
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

/// 放大镜连接线预览的几何与样式参数。
pub(super) struct MagnifierConnectorPreviewSpec {
    pub x0: f32,
    pub x1: f32,
    pub center_y: f32,
    pub scale: f32,
    pub connector: MagnifierConnector,
    pub color: Color,
}

/// 绘制放大镜连接线图示：用两个小框表示源区域与放大图像，并展示当前连接方式。
pub(super) fn render_magnifier_connector_preview(
    pixmap: &mut PixmapMut,
    spec: MagnifierConnectorPreviewSpec,
) {
    let box_size = 7.0 * spec.scale;
    let left = spec.x0;
    let right = spec.x1 - box_size;
    let top = spec.center_y - box_size / 2.0;
    let paint = make_paint(spec.color);
    let box_stroke = Stroke {
        width: 1.2 * spec.scale,
        ..Default::default()
    };
    for x in [left, right] {
        let Some(rect) = tiny_skia::Rect::from_xywh(x, top, box_size, box_size) else {
            continue;
        };
        let mut builder = PathBuilder::new();
        builder.push_rect(rect);
        if let Some(path) = builder.finish() {
            pixmap.stroke_path(&path, &paint, &box_stroke, Transform::identity(), None);
        }
    }

    let line_start = left + box_size;
    let line_end = right;
    let mut line = PathBuilder::new();
    line.move_to(line_start, spec.center_y);
    line.line_to(line_end, spec.center_y);
    if let Some(path) = line.finish() {
        draw_connector_line(pixmap, &path, line_start, line_end, &spec, &paint);
    }
}

/// 按连接线模式绘制线段、框线端帽或叉线。
fn draw_connector_line(
    pixmap: &mut PixmapMut,
    path: &tiny_skia::Path,
    start: f32,
    end: f32,
    spec: &MagnifierConnectorPreviewSpec,
    paint: &Paint,
) {
    match spec.connector {
        MagnifierConnector::Solid => {
            LineStyle::Solid.stroke_path(pixmap, path, paint, 1.4 * spec.scale);
        }
        MagnifierConnector::Dotted => {
            LineStyle::Dotted.stroke_path(pixmap, path, paint, 1.4 * spec.scale);
        }
        MagnifierConnector::Frame => {
            LineStyle::Solid.stroke_path(pixmap, path, paint, 1.4 * spec.scale);
            draw_connector_caps(pixmap, start, end, spec, paint);
        }
        MagnifierConnector::None => draw_connector_cross(pixmap, (start + end) / 2.0, spec, paint),
    }
}

/// 绘制框线连接模式两端的竖向端帽。
fn draw_connector_caps(
    pixmap: &mut PixmapMut,
    start: f32,
    end: f32,
    spec: &MagnifierConnectorPreviewSpec,
    paint: &Paint,
) {
    let mut caps = PathBuilder::new();
    for x in [start, end] {
        caps.move_to(x, spec.center_y - 3.0 * spec.scale);
        caps.line_to(x, spec.center_y + 3.0 * spec.scale);
    }
    if let Some(caps) = caps.finish() {
        pixmap.stroke_path(
            &caps,
            paint,
            &Stroke {
                width: 1.2 * spec.scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// 绘制表示不连接的叉线。
fn draw_connector_cross(
    pixmap: &mut PixmapMut,
    center_x: f32,
    spec: &MagnifierConnectorPreviewSpec,
    paint: &Paint,
) {
    let size = 2.5 * spec.scale;
    let mut cross = PathBuilder::new();
    cross.move_to(center_x - size, spec.center_y - size);
    cross.line_to(center_x + size, spec.center_y + size);
    cross.move_to(center_x + size, spec.center_y - size);
    cross.line_to(center_x - size, spec.center_y + size);
    if let Some(cross) = cross.finish() {
        pixmap.stroke_path(
            &cross,
            paint,
            &Stroke {
                width: 1.2 * spec.scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}
