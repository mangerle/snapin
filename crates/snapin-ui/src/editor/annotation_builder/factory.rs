//! 标注草稿到持久化图元的纯构造逻辑。

use super::*;

/// 统一图元工厂：构建已完成绘制的持久化 AnnotationShape 图元
pub fn build_shape(params: &AnnotationDraftParams) -> Option<AnnotationShape> {
    let pts = params.points;
    let first = *pts.first()?;
    let last = *pts.last()?;
    if params.tool != Tool::Badge && pts.len() < 2 {
        return None;
    }
    match params.tool {
        Tool::Rect => Some(build_rect(params, first, last)),
        Tool::Ellipse => Some(build_ellipse(params, first, last)),
        Tool::Arrow => Some(build_arrow(params, first, last)),
        Tool::Pen => Some(build_pen(params)),
        Tool::Line => Some(build_polyline(params)),
        Tool::Marker => Some(build_marker(params)),
        Tool::Mosaic => build_mosaic(params),
        Tool::Blur => Some(build_blur_box(params, first, last)),
        Tool::Spotlight => Some(build_spotlight(params, first, last)),
        Tool::Magnifier => Some(build_magnifier(params, first, last)),
        Tool::Badge => Some(build_badge(params, last)),
        _ => None,
    }
}

fn build_rect(
    params: &AnnotationDraftParams,
    first: (f32, f32),
    last: (f32, f32),
) -> AnnotationShape {
    let rect = Rect::from_points(first, last);
    AnnotationShape::Rect(RectAction {
        rect: (rect.x, rect.y, rect.width, rect.height),
        color: params.style.color_rgba.into(),
        stroke_width: params.style.stroke_width,
        is_filled: params.style.filled,
        corner_radius: params.style.corner_radius,
        line_style: params.style.line_style,
    })
}

fn build_ellipse(
    params: &AnnotationDraftParams,
    first: (f32, f32),
    last: (f32, f32),
) -> AnnotationShape {
    let rect = Rect::from_points(first, last);
    AnnotationShape::Ellipse(EllipseAction {
        rect: (rect.x, rect.y, rect.width, rect.height),
        color: params.style.color_rgba.into(),
        stroke_width: params.style.stroke_width,
        is_filled: params.style.filled,
        line_style: params.style.line_style,
    })
}

fn build_arrow(
    params: &AnnotationDraftParams,
    first: (f32, f32),
    last: (f32, f32),
) -> AnnotationShape {
    AnnotationShape::Arrow(ArrowAction {
        start: first,
        end: last,
        color: params.style.color_rgba.into(),
        stroke_width: params.style.stroke_width,
        style: params.style.arrow_style,
        line_style: params.style.line_style,
        start_head: params.style.start_arrow_head,
        end_head: params.style.end_arrow_head,
    })
}

fn build_pen(params: &AnnotationDraftParams) -> AnnotationShape {
    AnnotationShape::Pen(PenAction {
        points: params.points.to_vec(),
        color: params.style.color_rgba.into(),
        stroke_width: params.style.stroke_width,
        line_style: params.style.line_style,
    })
}

fn build_polyline(params: &AnnotationDraftParams) -> AnnotationShape {
    let (start_head, end_head) = if params.style.line_arrow_enabled {
        (params.style.start_arrow_head, params.style.end_arrow_head)
    } else {
        (ArrowHeadStyle::None, ArrowHeadStyle::None)
    };
    AnnotationShape::Polyline(PolylineAction {
        points: params.points.to_vec(),
        color: params.style.color_rgba.into(),
        stroke_width: params.style.stroke_width,
        line_style: params.style.line_style,
        start_head,
        end_head,
    })
}

fn build_marker(params: &AnnotationDraftParams) -> AnnotationShape {
    AnnotationShape::Marker(MarkerAction {
        points: params.points.to_vec(),
        color: (
            params.style.color_rgba.0,
            params.style.color_rgba.1,
            params.style.color_rgba.2,
        ),
        stroke_width: params.style.stroke_width * 2.0,
        shape: params.style.marker_shape,
        blend_mode: params.style.marker_blend_mode,
        line_style: params.style.line_style,
    })
}

fn build_mosaic(params: &AnnotationDraftParams) -> Option<AnnotationShape> {
    match params.style.mosaic_mode {
        MosaicMode::Mosaic => build_mosaic_box(params),
        MosaicMode::Blur => build_blur_brush(params),
        MosaicMode::SmartRedact => None,
    }
}

fn brush_bounds(params: &AnnotationDraftParams) -> Option<(Rect, f32)> {
    let rect = points_bounds(params.points)?;
    let brush_width = params.style.stroke_width.max(1.0);
    Some((
        Rect {
            x: rect.x - brush_width / 2.0,
            y: rect.y - brush_width / 2.0,
            width: rect.width + brush_width,
            height: rect.height + brush_width,
        },
        brush_width,
    ))
}

fn brush_points(params: &AnnotationDraftParams) -> Vec<(f32, f32)> {
    if params.style.mosaic_brush_mode {
        params.points.to_vec()
    } else {
        Vec::new()
    }
}

fn brush_width(params: &AnnotationDraftParams, width: f32) -> f32 {
    if params.style.mosaic_brush_mode {
        width
    } else {
        0.0
    }
}

fn build_mosaic_box(params: &AnnotationDraftParams) -> Option<AnnotationShape> {
    let (rect, width) = brush_bounds(params)?;
    Some(AnnotationShape::Mosaic(MosaicAction {
        rect: (rect.x, rect.y, rect.width, rect.height),
        cell_size: (f32::from(params.style.effect_strength) / 4.0).clamp(4.0, 50.0) as u32,
        points: brush_points(params),
        brush_width: brush_width(params, width),
    }))
}

fn build_blur_brush(params: &AnnotationDraftParams) -> Option<AnnotationShape> {
    let (rect, width) = brush_bounds(params)?;
    Some(AnnotationShape::Blur(BlurAction {
        rect: (rect.x, rect.y, rect.width, rect.height),
        radius: (f32::from(params.style.effect_strength) / 4.0).clamp(2.0, 50.0) as u32,
        points: brush_points(params),
        brush_width: brush_width(params, width),
    }))
}

fn build_blur_box(
    params: &AnnotationDraftParams,
    first: (f32, f32),
    last: (f32, f32),
) -> AnnotationShape {
    let rect = Rect::from_points(first, last);
    AnnotationShape::Blur(BlurAction {
        rect: (rect.x, rect.y, rect.width, rect.height),
        radius: (f32::from(params.style.effect_strength) / 4.0).clamp(2.0, 50.0) as u32,
        points: Vec::new(),
        brush_width: 0.0,
    })
}

fn build_spotlight(
    params: &AnnotationDraftParams,
    first: (f32, f32),
    last: (f32, f32),
) -> AnnotationShape {
    let rect = Rect::from_points(first, last);
    AnnotationShape::Spotlight(SpotlightAction {
        rect: (rect.x, rect.y, rect.width, rect.height),
        intensity: params.style.spotlight_intensity,
        shape: params.style.spotlight_shape,
        outline: params.style.spotlight_outline,
        outline_width: params.style.stroke_width,
        outline_color: params.style.color_rgba.into(),
        line_style: params.style.line_style,
    })
}

fn build_magnifier(
    params: &AnnotationDraftParams,
    first: (f32, f32),
    last: (f32, f32),
) -> AnnotationShape {
    let source = Rect::from_points(first, last);
    let scale = params.style.magnifier_scale.max(1.0);
    let dest = Rect {
        x: source.x + (source.width - source.width * scale) / 2.0,
        y: source.y + (source.height - source.height * scale) / 2.0,
        width: source.width * scale,
        height: source.height * scale,
    };
    AnnotationShape::Magnifier(MagnifierAction {
        source: (source.x, source.y, source.width, source.height),
        dest: (dest.x, dest.y, dest.width, dest.height),
        shape: params.style.magnifier_shape,
        border_width: params.style.stroke_width,
        border_color: params.style.color_rgba.into(),
        smooth: params.style.magnifier_smooth,
        shadow: params.style.magnifier_shadow,
        connector: params.style.magnifier_connector,
    })
}

fn build_badge(params: &AnnotationDraftParams, center: (f32, f32)) -> AnnotationShape {
    let radius = (params.style.stroke_width * 3.0).clamp(10.0, 48.0) * params.style.ui_scale;
    AnnotationShape::Badge(BadgeAction {
        center,
        radius,
        number: params.badge_number as usize,
        color: params.style.color_rgba.into(),
        style: params.style.badge_style,
        sequence: params.style.badge_sequence,
        ..BadgeAction::default()
    })
}
