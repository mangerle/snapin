//! 可复用标注图元工厂与即时预览渲染。

use crate::editor::toolbar::{MosaicMode, Tool, Toolbar};
use crate::geometry::Rect;
use snapin_annotation::action::AnnotationShape;
use snapin_annotation::shapes::{
    ArrowAction, BadgeAction, BlurAction, EllipseAction, MagnifierAction, MagnifierConnector,
    MagnifierShape, MarkerAction, MarkerBlendMode, MarkerShape, MosaicAction, PenAction,
    PolylineAction, RectAction, SpotlightAction, SpotlightShape,
};
use snapin_annotation::{ArrowHeadStyle, ArrowStyle, BadgeSequence, BadgeStyle, LineStyle};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, StrokeDash, Transform};

mod factory;
pub use factory::build_shape;

/// 工具栏样式快照：与几何点和序号解耦，预览与最终提交共享同一份样式。
#[derive(Debug, Clone, Copy)]
pub struct AnnotationStyle {
    pub stroke_width: f32,
    pub color_rgba: (u8, u8, u8, u8),
    pub ui_scale: f32,
    pub line_style: LineStyle,
    pub filled: bool,
    pub corner_radius: f32,
    pub badge_style: BadgeStyle,
    pub badge_sequence: BadgeSequence,
    pub arrow_style: ArrowStyle,
    pub start_arrow_head: ArrowHeadStyle,
    pub end_arrow_head: ArrowHeadStyle,
    pub spotlight_intensity: u8,
    pub spotlight_shape: SpotlightShape,
    pub spotlight_outline: bool,
    pub marker_shape: MarkerShape,
    pub marker_blend_mode: MarkerBlendMode,
    pub mosaic_mode: MosaicMode,
    pub mosaic_brush_mode: bool,
    pub effect_strength: u8,
    pub line_arrow_enabled: bool,
    pub magnifier_shape: MagnifierShape,
    pub magnifier_scale: f32,
    pub magnifier_smooth: bool,
    pub magnifier_shadow: bool,
    pub magnifier_connector: MagnifierConnector,
}

impl AnnotationStyle {
    /// 从当前工具栏控件状态固化样式。
    pub fn from_toolbar(toolbar: &Toolbar) -> Self {
        let color = toolbar.sub_options.color;
        Self {
            stroke_width: toolbar.sub_options.current_stroke_width(),
            color_rgba: (
                (color.red() * 255.0).round() as u8,
                (color.green() * 255.0).round() as u8,
                (color.blue() * 255.0).round() as u8,
                (color.alpha() * 255.0).round() as u8,
            ),
            ui_scale: toolbar.ui_scale,
            line_style: toolbar.sub_options.line_style,
            filled: toolbar.sub_options.filled,
            corner_radius: toolbar.sub_options.corner_radius,
            badge_style: toolbar.sub_options.badge_style,
            badge_sequence: toolbar.sub_options.badge_sequence,
            arrow_style: toolbar.sub_options.arrow_style,
            start_arrow_head: toolbar.sub_options.start_arrow_head,
            end_arrow_head: toolbar.sub_options.end_arrow_head,
            spotlight_intensity: toolbar.sub_options.spotlight_intensity,
            spotlight_shape: toolbar.sub_options.spotlight_shape,
            spotlight_outline: toolbar.sub_options.spotlight_outline,
            marker_shape: toolbar.sub_options.marker_shape,
            marker_blend_mode: toolbar.sub_options.marker_blend_mode,
            mosaic_mode: toolbar.sub_options.mosaic_mode,
            mosaic_brush_mode: toolbar.sub_options.mosaic_brush_mode,
            effect_strength: toolbar.sub_options.effect_strength,
            line_arrow_enabled: toolbar.sub_options.line_arrow_enabled,
            magnifier_shape: toolbar.sub_options.magnifier_shape,
            magnifier_scale: toolbar.sub_options.magnifier_scale,
            magnifier_smooth: toolbar.sub_options.magnifier_smooth,
            magnifier_shadow: toolbar.sub_options.magnifier_shadow,
            magnifier_connector: toolbar.sub_options.magnifier_connector,
        }
    }
}

/// 即时预览与提交时的几何与样式上下文参数
pub struct AnnotationDraftParams<'a> {
    pub tool: Tool,
    pub points: &'a [(f32, f32)],
    pub badge_number: u32,
    pub style: AnnotationStyle,
}

impl<'a> AnnotationDraftParams<'a> {
    /// 从画布渲染即时上下文构建参数
    pub fn from_toolbar_and_points(
        toolbar: &Toolbar,
        tool: Tool,
        points: &'a [(f32, f32)],
        badge_number: u32,
    ) -> Option<Self> {
        if points.is_empty() {
            return None;
        }
        if tool != Tool::Badge && points.len() < 2 {
            return None;
        }

        Some(Self {
            tool,
            points,
            badge_number,
            style: AnnotationStyle::from_toolbar(toolbar),
        })
    }
}

/// 统一图元工厂：直接在目标 Pixmap 上即时光栅化渲染正在交互绘制的预览图元
///
/// 核心优化：
/// 画笔 (Pen) 与记号笔 (Marker) 直接借用点集切片 `&[(f32, f32)]` 进行光栅化绘制，
/// 彻底根除在鼠标 60Hz~144Hz 高频移动事件中频繁触发的 `pts.to_vec()` 堆内存分配。
pub fn render_draft_preview(params: &AnnotationDraftParams, pixmap: &mut PixmapMut) {
    match params.tool {
        Tool::Pen => draw_pen_preview(params, pixmap),
        Tool::Line => draw_shape_with_border(params, pixmap),
        Tool::Marker => draw_marker_preview(params, pixmap),
        Tool::Blur => draw_blur_preview(params, pixmap),
        Tool::Mosaic => draw_mosaic_preview(params, pixmap),
        _ => draw_shape_preview(params, pixmap),
    }
}

/// 画笔直接借用点集切片绘制，避免高频鼠标移动时分配临时 Vec。
fn draw_pen_preview(params: &AnnotationDraftParams, pixmap: &mut PixmapMut) {
    PenAction::draw_points_offset(
        params.points,
        params.style.color_rgba,
        params.style.stroke_width,
        params.style.line_style,
        pixmap,
        0.0,
        0.0,
    );
}

/// 直线预览使用最终图元，并补一圈虚线边框提示拖拽范围。
fn draw_shape_with_border(params: &AnnotationDraftParams, pixmap: &mut PixmapMut) {
    draw_shape_preview(params, pixmap);
    draw_draft_border(params, pixmap);
}

/// 记号笔直接借用点集切片绘制。
fn draw_marker_preview(params: &AnnotationDraftParams, pixmap: &mut PixmapMut) {
    MarkerAction::draw_points_offset(
        params.points,
        (
            params.style.color_rgba.0,
            params.style.color_rgba.1,
            params.style.color_rgba.2,
        ),
        params.style.stroke_width * 2.0,
        pixmap,
        0.0,
        0.0,
    );
}

/// 拖拽模糊只显示占位框，避免每帧执行大面积卷积。
fn draw_blur_preview(params: &AnnotationDraftParams, pixmap: &mut PixmapMut) {
    draw_blur_placeholder(params, pixmap);
    draw_draft_border(params, pixmap);
}

/// 马赛克按模式选择实时块化、画笔模糊或智能打码边框预览。
fn draw_mosaic_preview(params: &AnnotationDraftParams, pixmap: &mut PixmapMut) {
    match params.style.mosaic_mode {
        MosaicMode::Mosaic => {
            draw_shape_preview(params, pixmap);
            if !params.style.mosaic_brush_mode {
                draw_draft_border(params, pixmap);
            }
        }
        MosaicMode::Blur if params.style.mosaic_brush_mode => draw_shape_preview(params, pixmap),
        MosaicMode::Blur => draw_blur_preview(params, pixmap),
        MosaicMode::SmartRedact => draw_draft_border(params, pixmap),
    }
}

/// 其余图元复用同一构建工厂，保证预览与最终提交的几何和样式一致。
fn draw_shape_preview(params: &AnnotationDraftParams, pixmap: &mut PixmapMut) {
    if let Some(shape) = build_shape(params) {
        shape.draw(pixmap);
    }
}

/// 绘制模糊拖拽预览：半透明灰框代替实时卷积。
fn draw_blur_placeholder(params: &AnnotationDraftParams, pixmap: &mut PixmapMut) {
    let Some(first) = params.points.first().copied() else {
        return;
    };
    let Some(last) = params.points.last().copied() else {
        return;
    };
    let rect = Rect::from_points(first, last);
    let Some(rect) = tiny_skia::Rect::from_xywh(rect.x, rect.y, rect.width, rect.height) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(110, 110, 110, 120);
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
}

/// 为马赛克/模糊的拖拽预览绘制虚线边框。
///
/// 设计原理：这两种图元本身没有可见的主色，模糊预览又只是一块半透明灰，
/// 拖拽时若没有描边，用户很难判断当前框选的边界。描边只存在于即时预览层，
/// 提交后的图元只重放块化/卷积效果，不含任何描边，最终截图保持干净。
fn draw_draft_border(params: &AnnotationDraftParams, pixmap: &mut PixmapMut) {
    let Some(first) = params.points.first().copied() else {
        return;
    };
    let Some(last) = params.points.last().copied() else {
        return;
    };
    let rect = Rect::from_points(first, last);
    let scale = params.style.ui_scale.max(0.1);
    let stroke_width = (1.2 * scale).max(1.0);
    // 虚线路径内缩半个线宽，使描边完全落在拖拽矩形内部：
    // 直接按原边界描边会被矩形内外各分走一半像素覆盖，抗锯齿后亮度只剩约一半。
    let inset = stroke_width / 2.0;
    let inner_width = rect.width - stroke_width;
    let inner_height = rect.height - stroke_width;
    if inner_width <= 0.0 || inner_height <= 0.0 {
        return;
    }
    let Some(skia_rect) =
        tiny_skia::Rect::from_xywh(rect.x + inset, rect.y + inset, inner_width, inner_height)
    else {
        return;
    };

    // 白色虚线，与标注选中框保持同一视觉语言，且在深浅背景上均可辨认。
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 235);
    let stroke = Stroke {
        width: stroke_width,
        dash: StrokeDash::new(vec![6.0 * scale, 4.0 * scale], 0.0),
        ..Default::default()
    };

    let mut pb = PathBuilder::new();
    pb.push_rect(skia_rect);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 计算点集轴对齐包围盒；调用前已保证至少两个点。
fn points_bounds(points: &[(f32, f32)]) -> Option<Rect> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (x, y) in points {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    (!points.is_empty()).then_some(Rect {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(1.0),
        height: (max_y - min_y).max(1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params<'a>(tool: Tool, points: &'a [(f32, f32)]) -> AnnotationDraftParams<'a> {
        AnnotationDraftParams {
            tool,
            points,
            badge_number: 1,
            style: AnnotationStyle {
                stroke_width: 4.0,
                color_rgba: (255, 0, 0, 255),
                ui_scale: 1.0,
                line_style: LineStyle::Solid,
                filled: false,
                corner_radius: 0.0,
                badge_style: BadgeStyle::Filled,
                badge_sequence: BadgeSequence::Number,
                arrow_style: ArrowStyle::Tapered,
                start_arrow_head: ArrowHeadStyle::None,
                end_arrow_head: ArrowHeadStyle::Triangle,
                spotlight_intensity: 160,
                spotlight_shape: SpotlightShape::Rectangle,
                spotlight_outline: false,
                marker_shape: MarkerShape::Stroke,
                marker_blend_mode: MarkerBlendMode::Multiply,
                mosaic_mode: MosaicMode::Mosaic,
                mosaic_brush_mode: false,
                effect_strength: 64,
                line_arrow_enabled: true,
                magnifier_shape: MagnifierShape::Rectangle,
                magnifier_scale: 2.0,
                magnifier_smooth: true,
                magnifier_shadow: true,
                magnifier_connector: MagnifierConnector::Solid,
            },
        }
    }

    /// 空点集与单点几何必须返回 None，而不是在工厂内部越界索引。
    #[test]
    fn test_build_shape_contract_rejects_insufficient_points() {
        assert!(build_shape(&params(Tool::Rect, &[])).is_none());
        assert!(build_shape(&params(Tool::Rect, &[(1.0, 2.0)])).is_none());
        assert!(build_shape(&params(Tool::Arrow, &[(1.0, 2.0)])).is_none());
        assert!(build_shape(&params(Tool::Pen, &[(1.0, 2.0)])).is_none());
    }

    /// 马赛克网格密度必须来自二级栏尺寸参数，不能继续写死为 10。
    #[test]
    fn test_mosaic_uses_selected_cell_size() {
        let mut draft = params(Tool::Mosaic, &[(0.0, 0.0), (20.0, 20.0)]);
        draft.style.effect_strength = 64;
        match build_shape(&draft) {
            Some(AnnotationShape::Mosaic(action)) => assert_eq!(action.cell_size, 16),
            other => panic!("应生成马赛克图元，实际: {other:?}"),
        }
    }

    /// 序号工具允许单点定位，仍应生成图元。
    #[test]
    fn test_build_shape_accepts_single_badge_point() {
        assert!(build_shape(&params(Tool::Badge, &[(10.0, 20.0)])).is_some());
    }

    /// 所有提供线型控件的工具都必须把所选线型写入最终图元。
    #[test]
    fn test_line_style_is_applied_across_stroke_tools() {
        let mut style = params(Tool::Ellipse, &[]).style;
        style.line_style = LineStyle::Dashed;
        style.arrow_style = ArrowStyle::Solid;
        style.spotlight_outline = true;
        for tool in [
            Tool::Ellipse,
            Tool::Arrow,
            Tool::Pen,
            Tool::Line,
            Tool::Spotlight,
        ] {
            let points = [(10.0, 10.0), (80.0, 60.0)];
            let draft = AnnotationDraftParams {
                tool,
                points: &points,
                badge_number: 1,
                style,
            };
            let shape = build_shape(&draft).expect("应生成图元");
            let actual = match shape {
                AnnotationShape::Ellipse(action) => action.line_style,
                AnnotationShape::Arrow(action) => action.line_style,
                AnnotationShape::Pen(action) => action.line_style,
                AnnotationShape::Polyline(action) => action.line_style,
                AnnotationShape::Spotlight(action) => action.line_style,
                other => panic!("不应生成该图元: {other:?}"),
            };
            assert_eq!(actual, LineStyle::Dashed, "{tool:?} 未应用所选线型");
        }
    }

    /// 马赛克与模糊在拖拽预览阶段必须有可见边框，提交后的图元不得保留描边。
    #[test]
    fn test_mosaic_and_blur_border_only_in_draft_preview() {
        use tiny_skia::{Color, Pixmap};

        for tool in [Tool::Mosaic, Tool::Blur] {
            let points = [(10.0, 10.0), (60.0, 50.0)];
            let draft = params(tool, &points);

            let mut preview = Pixmap::new(80, 70).expect("创建预览位图失败");
            preview.fill(Color::from_rgba8(10, 20, 30, 255));
            let mut preview_mut = preview.as_mut();
            render_draft_preview(&draft, &mut preview_mut);
            assert!(
                has_bright_pixel_near_edges(&preview, 10, 10, 50, 40),
                "{tool:?} 拖拽预览必须显示白色虚线边框"
            );

            let mut committed = Pixmap::new(80, 70).expect("创建提交位图失败");
            committed.fill(Color::from_rgba8(10, 20, 30, 255));
            let mut committed_mut = committed.as_mut();
            build_shape(&draft)
                .expect("应能构建持久化图元")
                .draw(&mut committed_mut);
            assert!(
                !has_bright_pixel_near_edges(&committed, 10, 10, 50, 40),
                "{tool:?} 提交后不得保留任何描边"
            );
        }
    }

    /// 沿矩形四条边外扩一像素扫描是否存在接近白色的像素，兼容抗锯齿与虚线间隙。
    fn has_bright_pixel_near_edges(
        pixmap: &tiny_skia::Pixmap,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> bool {
        let is_bright = |px: &[u8]| px[0] > 180 && px[1] > 180 && px[2] > 180;
        let right = x + width;
        let bottom = y + height;
        let samples = (0..=(bottom - y)).flat_map(|offset| {
            let py = y + offset;
            [x - 1, x, x + 1, right - 1, right, right + 1].map(|px| (px, py))
        });
        let horizontal = (0..=(right - x)).flat_map(|offset| {
            let px = x + offset;
            [y - 1, y, y + 1, bottom - 1, bottom, bottom + 1].map(|py| (px, py))
        });
        samples.chain(horizontal).any(|(px, py)| {
            if px < 0 || py < 0 || px >= pixmap.width() as i32 || py >= pixmap.height() as i32 {
                return false;
            }
            let index = ((py as u32 * pixmap.width() + px as u32) * 4) as usize;
            is_bright(&pixmap.data()[index..index + 4])
        })
    }
}
