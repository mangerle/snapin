//! 二级栏具体语义图标绘制。

use super::super::super::icons::make_paint;
use super::super::super::items::{LineMode, MosaicMode};
use super::common::{
    push_glyph_a, push_rect, push_rounded_rect, render_rectangle_icon, stroke_builder, with_alpha,
    IconRect, SubBarIconSpec,
};
use snapin_annotation::{BadgeSequence, MarkerBlendMode, MarkerShape};
use snapin_render::{with_thread_local_text_renderer, CenteredTextOptions};
use tiny_skia::{PathBuilder, PixmapMut, Transform};

/// 绘制直线或折线模式图标。
pub(in crate::editor::toolbar::render) fn render_line_mode_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
    mode: LineMode,
) {
    let mut builder = PathBuilder::new();
    match mode {
        LineMode::Straight => {
            let (x0, y0) = spec.point(-7.0, 5.0);
            let (x1, y1) = spec.point(7.0, -5.0);
            builder.move_to(x0, y0);
            builder.line_to(x1, y1);
        }
        LineMode::Polyline => {
            let points = [
                spec.point(-7.0, 5.0),
                spec.point(-2.0, -5.0),
                spec.point(2.0, 1.0),
                spec.point(7.0, -5.0),
            ];
            builder.move_to(points[0].0, points[0].1);
            for point in &points[1..] {
                builder.line_to(point.0, point.1);
            }
        }
    }
    stroke_builder(pixmap, builder, spec, 1.8);
}

/// 绘制箭头总开关图标；关闭态增加斜杠明确禁用语义。
pub(in crate::editor::toolbar::render) fn render_arrow_toggle_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
    enabled: bool,
) {
    let mut builder = PathBuilder::new();
    let (x0, y) = spec.point(-7.0, 0.0);
    let (x1, _) = spec.point(6.5, 0.0);
    builder.move_to(x0, y);
    builder.line_to(x1, y);
    let (tip_x, tip_y) = spec.point(7.0, 0.0);
    let (base_x, upper_y) = spec.point(2.0, -3.5);
    let (_, lower_y) = spec.point(2.0, 3.5);
    builder.move_to(tip_x, tip_y);
    builder.line_to(base_x, upper_y);
    builder.line_to(base_x, lower_y);
    builder.close();
    if !enabled {
        let (slash_x0, slash_y0) = spec.point(-7.0, 7.0);
        let (slash_x1, slash_y1) = spec.point(7.0, -7.0);
        builder.move_to(slash_x0, slash_y0);
        builder.line_to(slash_x1, slash_y1);
    }
    stroke_builder(pixmap, builder, spec, 1.6);
}

/// 绘制橡皮擦的画笔或框选模式图标。
pub(in crate::editor::toolbar::render) fn render_eraser_mode_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
    rectangle: bool,
) {
    if rectangle {
        render_rectangle_icon(pixmap, spec, true);
        return;
    }
    let mut builder = PathBuilder::new();
    let (x0, y0) = spec.point(-7.0, 6.5);
    let (x1, y1) = spec.point(4.5, -6.5);
    let (x2, y2) = spec.point(7.0, -4.0);
    let (x3, y3) = spec.point(-4.5, 9.0);
    builder.move_to(x0, y0);
    builder.line_to(x1, y1);
    builder.line_to(x2, y2);
    builder.line_to(x3, y3);
    builder.close();
    stroke_builder(pixmap, builder, spec, 1.6);
}

/// 绘制清空全部标注的垃圾桶图标。
pub(in crate::editor::toolbar::render) fn render_clear_annotations_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
) {
    let mut builder = PathBuilder::new();
    let (left, top) = spec.point(-5.0, -6.0);
    let (right, _) = spec.point(5.0, -6.0);
    builder.move_to(left, top);
    builder.line_to(right, top);
    let (handle_left, handle_top) = spec.point(-2.5, -8.0);
    let (handle_right, handle_bottom) = spec.point(2.5, -6.0);
    builder.move_to(handle_left, handle_top);
    builder.line_to(handle_right, handle_bottom);
    let (body_left, body_top) = spec.point(-4.0, -4.0);
    let (body_right, body_bottom) = spec.point(4.0, 7.5);
    builder.move_to(body_left, body_top);
    builder.line_to(body_right, body_bottom);
    let (inner_x, inner_top) = spec.point(0.0, -2.0);
    let (_, inner_bottom) = spec.point(0.0, 5.0);
    builder.move_to(inner_x, inner_top);
    builder.line_to(inner_x, inner_bottom);
    stroke_builder(pixmap, builder, spec, 1.6);
}

/// 绘制文字描边开关图标。
pub(in crate::editor::toolbar::render) fn render_text_stroke_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
) {
    let mut builder = PathBuilder::new();
    push_glyph_a(&mut builder, spec, 8.0, 6.5);
    stroke_builder(pixmap, builder, spec, 1.7);
}

/// 绘制文字背景开关图标。
pub(in crate::editor::toolbar::render) fn render_text_background_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
) {
    let mut frame = PathBuilder::new();
    push_rounded_rect(
        &mut frame,
        spec,
        IconRect {
            x: -8.0,
            y: -7.0,
            width: 16.0,
            height: 14.0,
        },
        2.5,
    );
    if let Some(path) = frame.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(with_alpha(spec.color, 42)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    let mut glyph = PathBuilder::new();
    push_glyph_a(&mut glyph, spec, 6.5, 5.2);
    stroke_builder(pixmap, glyph, spec, 1.5);
}

/// 绘制透明度图标，使用明暗对半的圆表示混合程度。
pub(in crate::editor::toolbar::render) fn render_opacity_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
) {
    let mut outline = PathBuilder::new();
    outline.push_circle(spec.center_x, spec.center_y, 6.5 * spec.scale);
    stroke_builder(pixmap, outline, spec, 1.5);
    let mut fill = PathBuilder::new();
    let (left, top) = spec.point(-6.0, -6.0);
    let (right, bottom) = spec.point(6.0, 6.0);
    fill.move_to(spec.center_x, spec.center_y);
    fill.line_to(left, top);
    fill.line_to(left, bottom);
    fill.close();
    fill.move_to(spec.center_x, spec.center_y);
    fill.line_to(right, top);
    fill.line_to(right, bottom);
    fill.close();
    if let Some(path) = fill.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(spec.color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制文字背景内边距图标。
pub(in crate::editor::toolbar::render) fn render_padding_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
) {
    let mut outer = PathBuilder::new();
    push_rounded_rect(
        &mut outer,
        spec,
        IconRect {
            x: -8.0,
            y: -7.0,
            width: 16.0,
            height: 14.0,
        },
        2.0,
    );
    stroke_builder(pixmap, outer, spec, 1.3);
    let mut inner = PathBuilder::new();
    push_rounded_rect(
        &mut inner,
        spec,
        IconRect {
            x: -4.5,
            y: -3.5,
            width: 9.0,
            height: 7.0,
        },
        1.0,
    );
    stroke_builder(pixmap, inner, spec, 1.0);
}

/// 绘制聚光灯描边开关图标。
pub(in crate::editor::toolbar::render) fn render_spotlight_outline_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
) {
    let mut builder = PathBuilder::new();
    push_rounded_rect(
        &mut builder,
        spec,
        IconRect {
            x: -7.0,
            y: -6.0,
            width: 14.0,
            height: 12.0,
        },
        3.0,
    );
    stroke_builder(pixmap, builder, spec, 1.5);
    let mut center = PathBuilder::new();
    center.push_circle(spec.center_x, spec.center_y, 2.5 * spec.scale);
    stroke_builder(pixmap, center, spec, 1.2);
}

/// 绘制荧光笔线条或矩形模式图标。
pub(in crate::editor::toolbar::render) fn render_marker_shape_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
    shape: MarkerShape,
) {
    match shape {
        MarkerShape::Stroke => {
            let mut builder = PathBuilder::new();
            let (x0, y0) = spec.point(-7.0, 5.0);
            let (x1, y1) = spec.point(7.0, -5.0);
            builder.move_to(x0, y0);
            builder.line_to(x1, y1);
            stroke_builder(pixmap, builder, spec, 3.0);
        }
        MarkerShape::Rectangle => render_rectangle_icon(pixmap, spec, false),
    }
}

/// 绘制荧光笔正片叠底或普通混合图标。
pub(in crate::editor::toolbar::render) fn render_marker_blend_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
    mode: MarkerBlendMode,
) {
    let radius = 5.0 * spec.scale;
    let offset = 3.0 * spec.scale;
    let mut back = PathBuilder::new();
    back.push_circle(spec.center_x - offset, spec.center_y, radius);
    if let Some(path) = back.finish() {
        let alpha = if mode == MarkerBlendMode::Multiply {
            110
        } else {
            190
        };
        pixmap.fill_path(
            &path,
            &make_paint(with_alpha(spec.color, alpha)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    let mut front = PathBuilder::new();
    front.push_circle(spec.center_x + offset, spec.center_y, radius);
    stroke_builder(pixmap, front, spec, 1.5);
}

/// 绘制马赛克、模糊或智能打码模式图标。
pub(in crate::editor::toolbar::render) fn render_mosaic_mode_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
    mode: MosaicMode,
) {
    match mode {
        MosaicMode::Mosaic => render_mosaic_grid_icon(pixmap, spec),
        MosaicMode::Blur => render_blur_mode_icon(pixmap, spec),
        MosaicMode::SmartRedact => render_smart_redact_icon(pixmap, spec),
    }
}

fn render_mosaic_grid_icon(pixmap: &mut PixmapMut, spec: SubBarIconSpec) {
    let mut frame = PathBuilder::new();
    push_rounded_rect(
        &mut frame,
        spec,
        IconRect {
            x: -7.0,
            y: -7.0,
            width: 14.0,
            height: 14.0,
        },
        1.5,
    );
    stroke_builder(pixmap, frame, spec, 1.4);
    for (dx, dy) in [(-3.5, -3.5), (3.5, 3.5)] {
        let mut square = PathBuilder::new();
        push_rect(
            &mut square,
            spec,
            IconRect {
                x: dx - 2.5,
                y: dy - 2.5,
                width: 5.0,
                height: 5.0,
            },
        );
        if let Some(path) = square.finish() {
            pixmap.fill_path(
                &path,
                &make_paint(spec.color),
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

fn render_blur_mode_icon(pixmap: &mut PixmapMut, spec: SubBarIconSpec) {
    let mut builder = PathBuilder::new();
    let (top_x, top_y) = spec.point(0.0, -7.5);
    builder.move_to(top_x, top_y);
    builder.cubic_to(
        spec.center_x + 6.0 * spec.scale,
        spec.center_y - 1.0 * spec.scale,
        spec.center_x + 5.5 * spec.scale,
        spec.center_y + 6.5 * spec.scale,
        spec.center_x,
        spec.center_y + 6.5 * spec.scale,
    );
    builder.cubic_to(
        spec.center_x - 5.5 * spec.scale,
        spec.center_y + 6.5 * spec.scale,
        spec.center_x - 6.0 * spec.scale,
        spec.center_y - 1.0 * spec.scale,
        top_x,
        top_y,
    );
    builder.close();
    stroke_builder(pixmap, builder, spec, 1.5);
}

fn render_smart_redact_icon(pixmap: &mut PixmapMut, spec: SubBarIconSpec) {
    let mut frame = PathBuilder::new();
    push_rounded_rect(
        &mut frame,
        spec,
        IconRect {
            x: -7.0,
            y: -5.0,
            width: 14.0,
            height: 10.0,
        },
        2.0,
    );
    stroke_builder(pixmap, frame, spec, 1.4);
    let mut slash = PathBuilder::new();
    let (x0, y0) = spec.point(-7.5, 6.5);
    let (x1, y1) = spec.point(7.5, -6.5);
    slash.move_to(x0, y0);
    slash.line_to(x1, y1);
    stroke_builder(pixmap, slash, spec, 1.7);
}

/// 绘制马赛克画笔或矩形选择图标。
pub(in crate::editor::toolbar::render) fn render_mosaic_shape_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
    brush: bool,
) {
    if brush {
        render_marker_shape_icon(pixmap, spec, MarkerShape::Stroke);
    } else {
        render_marker_shape_icon(pixmap, spec, MarkerShape::Rectangle);
    }
}

/// 绘制字体族图标，使用外框与字母 A 表示可选字体集合。
pub(in crate::editor::toolbar::render) fn render_font_family_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
) {
    let mut frame = PathBuilder::new();
    push_rounded_rect(
        &mut frame,
        spec,
        IconRect {
            x: -8.0,
            y: -7.0,
            width: 16.0,
            height: 14.0,
        },
        2.5,
    );
    stroke_builder(pixmap, frame, spec, 1.3);
    let mut glyph = PathBuilder::new();
    let glyph_spec = SubBarIconSpec {
        center_y: spec.center_y + 0.5 * spec.scale,
        scale: spec.scale * 0.82,
        ..spec
    };
    push_glyph_a(&mut glyph, glyph_spec, 9.0, 7.0);
    stroke_builder(pixmap, glyph, spec, 1.4);
}

/// 绘制字号图标，使用大小两个 A 表示尺度变化。
pub(in crate::editor::toolbar::render) fn render_font_size_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
) {
    let mut large = PathBuilder::new();
    let large_spec = SubBarIconSpec {
        center_x: spec.center_x - 2.5 * spec.scale,
        ..spec
    };
    push_glyph_a(&mut large, large_spec, 9.0, 6.5);
    stroke_builder(pixmap, large, spec, 1.5);
    let mut small = PathBuilder::new();
    let small_spec = SubBarIconSpec {
        center_x: spec.center_x + 5.0 * spec.scale,
        center_y: spec.center_y + 3.0 * spec.scale,
        scale: spec.scale * 0.62,
        ..spec
    };
    push_glyph_a(&mut small, small_spec, 9.0, 6.5);
    stroke_builder(pixmap, small, spec, 1.3);
}

/// 绘制放大镜抗锯齿开关图标。
pub(in crate::editor::toolbar::render) fn render_magnifier_smooth_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
) {
    let mut lens = PathBuilder::new();
    lens.push_circle(
        spec.center_x - 1.5 * spec.scale,
        spec.center_y - 1.5 * spec.scale,
        5.5 * spec.scale,
    );
    let (handle_x0, handle_y0) = spec.point(2.5, 2.5);
    let (handle_x1, handle_y1) = spec.point(7.0, 7.0);
    lens.move_to(handle_x0, handle_y0);
    lens.line_to(handle_x1, handle_y1);
    stroke_builder(pixmap, lens, spec, 1.5);
}

/// 绘制放大镜阴影开关图标。
pub(in crate::editor::toolbar::render) fn render_magnifier_shadow_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
) {
    let mut shadow = PathBuilder::new();
    shadow.push_circle(
        spec.center_x - 1.0 * spec.scale,
        spec.center_y - 0.5 * spec.scale,
        6.0 * spec.scale,
    );
    if let Some(path) = shadow.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(with_alpha(spec.color, 44)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    let mut lens = PathBuilder::new();
    lens.push_circle(
        spec.center_x - 2.0 * spec.scale,
        spec.center_y - 2.0 * spec.scale,
        5.0 * spec.scale,
    );
    stroke_builder(pixmap, lens, spec, 1.5);
}

/// 绘制序号格式图标，直接展示当前格式的第 1 个样本。
pub(in crate::editor::toolbar::render) fn render_badge_sequence_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
    sequence: BadgeSequence,
) {
    let label = sequence.label(1);
    let rgba = (
        (spec.color.red() * 255.0).round() as u8,
        (spec.color.green() * 255.0).round() as u8,
        (spec.color.blue() * 255.0).round() as u8,
        (spec.color.alpha() * 255.0).round() as u8,
    );
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_centered_bold_text(
            pixmap,
            &label,
            CenteredTextOptions {
                center: (spec.center_x, spec.center_y),
                font_size: 14.0 * spec.scale,
                max_extent: 18.0 * spec.scale,
                rgba,
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    /// 序号关闭态必须显示当前格式的真实样本，而不是固定文字或图标。
    #[test]
    fn test_badge_sequence_icon_uses_current_sample() {
        let render = |sequence| {
            let mut pixmap = Pixmap::new(32, 32).expect("测试位图应可分配");
            pixmap.fill(Color::TRANSPARENT);
            render_badge_sequence_icon(
                &mut pixmap.as_mut(),
                SubBarIconSpec {
                    center_x: 16.0,
                    center_y: 16.0,
                    scale: 1.0,
                    color: Color::from_rgba8(45, 48, 56, 255),
                },
                sequence,
            );
            pixmap.data().to_vec()
        };

        assert_ne!(
            render(BadgeSequence::Number),
            render(BadgeSequence::UppercaseLetter)
        );
    }
}
