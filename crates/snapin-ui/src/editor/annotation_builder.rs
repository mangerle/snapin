//! 可复用标注图元工厂与即时预览渲染。

use crate::editor::toolbar::{Tool, Toolbar};
use crate::geometry::Rect;
use snapin_core::annotation::action::AnnotationShape;
use snapin_core::annotation::shapes::{
    ArrowAction, BadgeAction, BlurAction, EllipseAction, MarkerAction, MosaicAction, PenAction,
    RectAction, SpotlightAction,
};
use snapin_core::annotation::{ArrowStyle, BadgeStyle, LineStyle};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, StrokeDash, Transform};

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
    pub arrow_style: ArrowStyle,
    pub spotlight_intensity: u8,
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
            arrow_style: toolbar.sub_options.arrow_style,
            spotlight_intensity: toolbar.sub_options.spotlight_intensity,
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
    let pts = params.points;
    match params.tool {
        // Pen/Marker 直接借用点集绘制，避免高频鼠标移动事件中的临时 Vec 分配。
        Tool::Pen => {
            PenAction::draw_points_offset(
                pts,
                params.style.color_rgba,
                params.style.stroke_width,
                pixmap,
                0.0,
                0.0,
            );
        }
        Tool::Marker => {
            MarkerAction::draw_points_offset(
                pts,
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
        // 模糊在拖拽过程中只显示半透明占位框，避免每帧对选区做大面积卷积；
        // 提交后由图元重放执行一次真正的模糊。
        Tool::Blur => {
            draw_blur_placeholder(params, pixmap);
            draw_draft_border(params, pixmap);
        }
        // 马赛克逐帧对目标区域执行真实块化；与模糊一样补一圈虚线边框，
        // 让拖拽范围在纹理复杂的画面上仍然可辨认。提交后仅重放块化效果。
        Tool::Mosaic => {
            if let Some(shape) = build_shape(params) {
                shape.draw(pixmap);
            }
            draw_draft_border(params, pixmap);
        }
        _ => {
            // 其余图元复用同一构建工厂，保证预览与最终提交的几何和样式完全一致。
            if let Some(shape) = build_shape(params) {
                shape.draw(pixmap);
            }
        }
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

/// 统一图元工厂：构建已完成绘制的持久化 AnnotationShape 图元
pub fn build_shape(params: &AnnotationDraftParams) -> Option<AnnotationShape> {
    let pts = params.points;
    let first = *pts.first()?;
    let last = *pts.last()?;
    if params.tool != Tool::Badge && pts.len() < 2 {
        return None;
    }
    match params.tool {
        Tool::Rect => {
            let rect = Rect::from_points(first, last);
            Some(AnnotationShape::Rect(RectAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                color: params.style.color_rgba.into(),
                stroke_width: params.style.stroke_width,
                is_filled: params.style.filled,
                corner_radius: params.style.corner_radius,
                line_style: params.style.line_style,
            }))
        }
        Tool::Ellipse => {
            let rect = Rect::from_points(first, last);
            Some(AnnotationShape::Ellipse(EllipseAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                color: params.style.color_rgba.into(),
                stroke_width: params.style.stroke_width,
                is_filled: params.style.filled,
                line_style: params.style.line_style,
            }))
        }
        Tool::Arrow => Some(AnnotationShape::Arrow(ArrowAction {
            start: first,
            end: last,
            color: params.style.color_rgba.into(),
            stroke_width: params.style.stroke_width,
            style: params.style.arrow_style,
        })),
        Tool::Pen => Some(AnnotationShape::Pen(PenAction {
            points: pts.to_vec(),
            color: params.style.color_rgba.into(),
            stroke_width: params.style.stroke_width,
        })),
        Tool::Marker => Some(AnnotationShape::Marker(MarkerAction {
            points: pts.to_vec(),
            color: (
                params.style.color_rgba.0,
                params.style.color_rgba.1,
                params.style.color_rgba.2,
            ),
            stroke_width: params.style.stroke_width * 2.0,
        })),
        Tool::Mosaic => {
            let rect = Rect::from_points(first, last);
            Some(AnnotationShape::Mosaic(MosaicAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                cell_size: (params.style.stroke_width * 2.0).clamp(4.0, 50.0) as u32,
            }))
        }
        Tool::Blur => {
            let rect = Rect::from_points(first, last);
            Some(AnnotationShape::Blur(BlurAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                radius: (params.style.stroke_width * 2.0).clamp(4.0, 50.0) as u32,
            }))
        }
        Tool::Spotlight => {
            let rect = Rect::from_points(first, last);
            Some(AnnotationShape::Spotlight(SpotlightAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                intensity: params.style.spotlight_intensity,
            }))
        }
        Tool::Badge => {
            let center = last;
            let radius =
                (params.style.stroke_width * 3.0).clamp(10.0, 48.0) * params.style.ui_scale;
            Some(AnnotationShape::Badge(BadgeAction {
                center,
                radius,
                number: params.badge_number as usize,
                color: params.style.color_rgba.into(),
                style: params.style.badge_style,
            }))
        }
        _ => None,
    }
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
                arrow_style: ArrowStyle::Tapered,
                spotlight_intensity: 160,
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
        draft.style.stroke_width = 8.0;
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
