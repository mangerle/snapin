//! 工具栏共享绘制原语：面板、分隔线、按钮高亮、圆形与属性预览图元。

mod arrow;

pub(super) use arrow::{
    render_arrow_head_preview, render_arrow_preview, ArrowHeadPreviewSpec, ArrowPreviewSpec,
};

use super::super::icons::{add_round_rect, make_paint, RoundRect};
use crate::geometry::Rect;
use snapin_annotation::{BadgeStyle, LineStyle, MagnifierConnector, StrokePathSpec};
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 横向预览图元的起止坐标、中心线、缩放与颜色。
#[derive(Clone, Copy)]
pub(super) struct PreviewSpan {
    pub(super) x0: f32,
    pub(super) x1: f32,
    pub(super) center_y: f32,
    pub(super) scale: f32,
    pub(super) color: Color,
}

/// 圆形图元的圆心、半径与颜色。
#[derive(Clone, Copy)]
pub(super) struct CircleSpec {
    pub(super) center_x: f32,
    pub(super) center_y: f32,
    pub(super) radius: f32,
    pub(super) color: Color,
}

/// 圆形描边图元的几何与线宽。
#[derive(Clone, Copy)]
pub(super) struct StrokeCircleSpec {
    pub(super) circle: CircleSpec,
    pub(super) width: f32,
}

/// 水平线型预览的几何与样式。
#[derive(Clone, Copy)]
pub(super) struct LineSampleSpec {
    pub(super) x0: f32,
    pub(super) x1: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) style: LineStyle,
    pub(super) color: Color,
}

/// 圆角半径图标的中心、缩放与当前值。
#[derive(Clone, Copy)]
pub(super) struct CornerRadiusIconSpec {
    pub(super) center_x: f32,
    pub(super) center_y: f32,
    pub(super) scale: f32,
    pub(super) corner_radius: f32,
}

/// 序号底板样式图标的中心、缩放、样式与颜色。
#[derive(Clone, Copy)]
pub(super) struct BadgeStyleIconSpec {
    pub(super) center_x: f32,
    pub(super) center_y: f32,
    pub(super) scale: f32,
    pub(super) style: BadgeStyle,
    pub(super) color: Color,
}

/// 绘制通用浅米白微圆角容器背景与微阴影 (参考官方经典现代浅色设计)
pub(super) fn render_panel_background(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
    draw_panel_shadow(pixmap, rect, s);
    draw_panel_body(pixmap, rect, s);
}

/// 绘制面板底部的浅淡投影。
fn draw_panel_shadow(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: rect.x,
            y: rect.y + 1.5 * s,
            width: rect.width,
            height: rect.height,
            radius: 6.0 * s,
        },
    );
    if let Some(path) = builder.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(0, 0, 0, 16)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制面板底色与细边框。
fn draw_panel_body(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            radius: 6.0 * s,
        },
    );
    let Some(path) = builder.finish() else {
        return;
    };
    pixmap.fill_path(
        &path,
        &make_paint(Color::from_rgba8(243, 244, 246, 252)),
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    let stroke = Stroke {
        width: 1.0 * s,
        ..Default::default()
    };
    pixmap.stroke_path(
        &path,
        &make_paint(Color::from_rgba8(0, 0, 0, 24)),
        &stroke,
        Transform::identity(),
        None,
    );
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

/// 按控件中心线绘制垂直分隔线。
///
/// `center_y` 是控件垂直中心而不是控件顶边；分隔线向上下对称延伸，
/// 避免主栏与二级栏因各自高度不同而出现偏下或不可辨认的问题。
pub(super) fn render_divider(pixmap: &mut PixmapMut, x: f32, center_y: f32, scale: f32) {
    let mut builder = PathBuilder::new();
    builder.move_to(x, center_y - 10.0 * scale);
    builder.line_to(x, center_y + 10.0 * scale);
    if let Some(path) = builder.finish() {
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(100, 116, 139, 190)),
            &Stroke {
                width: (1.25 * scale).max(1.0),
                line_cap: tiny_skia::LineCap::Round,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// 绘制一个实心圆点
pub(super) fn fill_circle(pixmap: &mut PixmapMut, circle: CircleSpec) {
    let mut pb = PathBuilder::new();
    pb.push_circle(circle.center_x, circle.center_y, circle.radius);
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(circle.color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制一个圆形描边
pub(super) fn stroke_circle(pixmap: &mut PixmapMut, spec: StrokeCircleSpec) {
    let circle = spec.circle;
    let mut pb = PathBuilder::new();
    pb.push_circle(circle.center_x, circle.center_y, circle.radius);
    if let Some(path) = pb.finish() {
        let stroke = Stroke {
            width: spec.width,
            ..Default::default()
        };
        pixmap.stroke_path(
            &path,
            &make_paint(circle.color),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

/// 按线型绘制一条水平示意线
pub(super) fn stroke_sample(pixmap: &mut PixmapMut, spec: LineSampleSpec) {
    let mut pb = PathBuilder::new();
    pb.move_to(spec.x0, spec.y);
    pb.line_to(spec.x1, spec.y);
    if let Some(path) = pb.finish() {
        spec.style.stroke_path(
            pixmap,
            &make_paint(spec.color),
            StrokePathSpec::new(&path, spec.width),
        );
    }
}

/// 绘制圆角半径图标：视觉圆角随当前半径值变化，滚轮调节时可直观看到效果
pub(super) fn render_corner_radius_icon(pixmap: &mut PixmapMut, spec: CornerRadiusIconSpec) {
    let (cx, cy, s) = (spec.center_x, spec.center_y, spec.scale);
    let half = 7.0 * s;
    let visual_radius = (1.0 + spec.corner_radius / 90.0 * 5.0) * s;
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
pub(super) fn render_badge_style_icon(pixmap: &mut PixmapMut, spec: BadgeStyleIconSpec) {
    let circle = CircleSpec {
        center_x: spec.center_x,
        center_y: spec.center_y,
        radius: 7.0 * spec.scale,
        color: spec.color,
    };
    match spec.style {
        BadgeStyle::Filled => fill_circle(pixmap, circle),
        BadgeStyle::Outline => stroke_circle(
            pixmap,
            StrokeCircleSpec {
                circle: CircleSpec {
                    radius: 6.5 * spec.scale,
                    ..circle
                },
                width: 2.0 * spec.scale,
            },
        ),
    }
}

/// 放大镜连接线预览的几何与样式参数。
pub(super) struct MagnifierConnectorPreviewSpec {
    pub span: PreviewSpan,
    pub connector: MagnifierConnector,
}

/// 绘制放大镜连接线图示：用两个小框表示源区域与放大图像，并展示当前连接方式。
pub(super) fn render_magnifier_connector_preview(
    pixmap: &mut PixmapMut,
    spec: MagnifierConnectorPreviewSpec,
) {
    let box_size = 7.0 * spec.span.scale;
    let left = spec.span.x0;
    let right = spec.span.x1 - box_size;
    let top = spec.span.center_y - box_size / 2.0;
    let paint = make_paint(spec.span.color);
    let box_stroke = Stroke {
        width: 1.2 * spec.span.scale,
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
    line.move_to(line_start, spec.span.center_y);
    line.line_to(line_end, spec.span.center_y);
    if let Some(path) = line.finish() {
        draw_connector_line(pixmap, &path, &spec, &paint);
    }
}

/// 按连接线模式绘制线段、框线端帽或叉线。
fn draw_connector_line(
    pixmap: &mut PixmapMut,
    path: &tiny_skia::Path,
    spec: &MagnifierConnectorPreviewSpec,
    paint: &Paint,
) {
    match spec.connector {
        MagnifierConnector::Solid => {
            LineStyle::Solid.stroke_path(
                pixmap,
                paint,
                StrokePathSpec::new(path, 1.4 * spec.span.scale),
            );
        }
        MagnifierConnector::Dotted => {
            LineStyle::Dotted.stroke_path(
                pixmap,
                paint,
                StrokePathSpec::new(path, 1.4 * spec.span.scale),
            );
        }
        MagnifierConnector::Frame => {
            LineStyle::Solid.stroke_path(
                pixmap,
                paint,
                StrokePathSpec::new(path, 1.4 * spec.span.scale),
            );
            draw_connector_caps(pixmap, spec, paint);
        }
        MagnifierConnector::None => draw_connector_cross(pixmap, spec, paint),
    }
}

/// 绘制框线连接模式两端的竖向端帽。
fn draw_connector_caps(
    pixmap: &mut PixmapMut,
    spec: &MagnifierConnectorPreviewSpec,
    paint: &Paint,
) {
    let box_size = 7.0 * spec.span.scale;
    let mut caps = PathBuilder::new();
    for x in [spec.span.x0 + box_size, spec.span.x1 - box_size] {
        caps.move_to(x, spec.span.center_y - 3.0 * spec.span.scale);
        caps.line_to(x, spec.span.center_y + 3.0 * spec.span.scale);
    }
    if let Some(caps) = caps.finish() {
        pixmap.stroke_path(
            &caps,
            paint,
            &Stroke {
                width: 1.2 * spec.span.scale,
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
    spec: &MagnifierConnectorPreviewSpec,
    paint: &Paint,
) {
    let center_x = (spec.span.x0 + spec.span.x1) / 2.0;
    let size = 2.5 * spec.span.scale;
    let mut cross = PathBuilder::new();
    cross.move_to(center_x - size, spec.span.center_y - size);
    cross.line_to(center_x + size, spec.span.center_y + size);
    cross.move_to(center_x + size, spec.span.center_y - size);
    cross.line_to(center_x - size, spec.span.center_y + size);
    if let Some(cross) = cross.finish() {
        pixmap.stroke_path(
            &cross,
            paint,
            &Stroke {
                width: 1.2 * spec.span.scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    /// 二级栏分隔线必须可见且围绕控件垂直中心展开，不能贴到底部。
    #[test]
    fn test_divider_is_visible_and_vertically_centered() {
        let mut pixmap = Pixmap::new(20, 40).expect("测试位图应可分配");
        pixmap.fill(Color::TRANSPARENT);
        render_divider(&mut pixmap.as_mut(), 10.0, 20.0, 1.0);

        let center = pixmap.pixel(10, 20).expect("中心像素应存在");
        let upper = pixmap.pixel(10, 12).expect("上部像素应存在");
        let below = pixmap.pixel(10, 8).expect("边界外像素应存在");
        assert!(center.alpha() > 0, "分隔线中心必须可见");
        assert!(upper.alpha() > 0, "分隔线必须向上对称延伸");
        assert_eq!(below.alpha(), 0, "分隔线不得溢出到控件外");
    }
}
