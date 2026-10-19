//! 标注动作图元工厂与提交逻辑实现。

use super::events::SnipperState;
use crate::annotation::action::{AnnotationAction, AnnotationShape};
use crate::annotation::shapes::{
    ArrowAction, BadgeAction, EllipseAction, MarkerAction, MosaicAction, PenAction, RectAction,
};
use crate::snipper::selection::Rect;
use crate::snipper::toolbar::{Toolbar, ToolbarItem};
use tiny_skia::PixmapMut;

/// 即时预览与提交时的几何与样式上下文参数
pub struct AnnotationDraftParams<'a> {
    pub tool: ToolbarItem,
    pub points: &'a [(f32, f32)],
    pub stroke_width: f32,
    pub color_rgba: (u8, u8, u8, u8),
    pub ui_scale: f32,
    pub badge_number: u32,
}

impl<'a> AnnotationDraftParams<'a> {
    /// 从当前编辑状态中提取参数
    pub fn from_state(state: &'a SnipperState) -> Option<Self> {
        let tool = state.toolbar.active_tool?;
        let pts = &state.pending_stroke_points;
        if pts.is_empty() {
            return None;
        }
        if tool != ToolbarItem::ToolBadge && pts.len() < 2 {
            return None;
        }

        let stroke_w = state.toolbar.sub_options.current_stroke_width();
        let c = state.toolbar.sub_options.color;
        let rgba = (
            (c.red() * 255.0).round() as u8,
            (c.green() * 255.0).round() as u8,
            (c.blue() * 255.0).round() as u8,
            (c.alpha() * 255.0).round() as u8,
        );

        Some(Self {
            tool,
            points: pts,
            stroke_width: stroke_w,
            color_rgba: rgba,
            ui_scale: state.toolbar.ui_scale,
            badge_number: state.next_badge_number,
        })
    }

    /// 从画布渲染即时上下文构建参数
    pub fn from_toolbar_and_points(
        toolbar: &Toolbar,
        tool: ToolbarItem,
        points: &'a [(f32, f32)],
        badge_number: u32,
    ) -> Option<Self> {
        if points.is_empty() {
            return None;
        }
        if tool != ToolbarItem::ToolBadge && points.len() < 2 {
            return None;
        }

        let stroke_w = toolbar.sub_options.current_stroke_width();
        let c = toolbar.sub_options.color;
        let rgba = (
            (c.red() * 255.0).round() as u8,
            (c.green() * 255.0).round() as u8,
            (c.blue() * 255.0).round() as u8,
            (c.alpha() * 255.0).round() as u8,
        );

        Some(Self {
            tool,
            points,
            stroke_width: stroke_w,
            color_rgba: rgba,
            ui_scale: toolbar.ui_scale,
            badge_number,
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
        ToolbarItem::ToolRect => {
            let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
            RectAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                color: params.color_rgba,
                stroke_width: params.stroke_width,
                is_filled: false,
            }
            .draw(pixmap);
        }
        ToolbarItem::ToolEllipse => {
            let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
            EllipseAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                color: params.color_rgba,
                stroke_width: params.stroke_width,
                is_filled: false,
            }
            .draw(pixmap);
        }
        ToolbarItem::ToolArrow => {
            ArrowAction {
                start: pts[0],
                end: pts[pts.len() - 1],
                color: params.color_rgba,
                stroke_width: params.stroke_width,
            }
            .draw(pixmap);
        }
        ToolbarItem::ToolPen => {
            PenAction::draw_points_offset(
                pts,
                params.color_rgba,
                params.stroke_width,
                pixmap,
                0.0,
                0.0,
            );
        }
        ToolbarItem::ToolMarker => {
            MarkerAction::draw_points_offset(
                pts,
                (
                    params.color_rgba.0,
                    params.color_rgba.1,
                    params.color_rgba.2,
                ),
                params.stroke_width * 2.0,
                pixmap,
                0.0,
                0.0,
            );
        }
        ToolbarItem::ToolMosaic => {
            let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
            MosaicAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                cell_size: 10,
            }
            .draw(pixmap);
        }
        ToolbarItem::ToolBadge => {
            let center = pts[pts.len() - 1];
            let radius = (params.stroke_width * 3.0).clamp(10.0, 48.0) * params.ui_scale;
            BadgeAction {
                center,
                radius,
                number: params.badge_number as usize,
                color: params.color_rgba,
            }
            .draw(pixmap);
        }
        _ => {}
    }
}

/// 统一图元工厂：构建已完成绘制的持久化 AnnotationShape 图元
pub fn build_shape(params: &AnnotationDraftParams) -> Option<AnnotationShape> {
    let pts = params.points;
    match params.tool {
        ToolbarItem::ToolRect => {
            let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
            Some(AnnotationShape::Rect(RectAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                color: params.color_rgba,
                stroke_width: params.stroke_width,
                is_filled: false,
            }))
        }
        ToolbarItem::ToolEllipse => {
            let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
            Some(AnnotationShape::Ellipse(EllipseAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                color: params.color_rgba,
                stroke_width: params.stroke_width,
                is_filled: false,
            }))
        }
        ToolbarItem::ToolArrow => Some(AnnotationShape::Arrow(ArrowAction {
            start: pts[0],
            end: pts[pts.len() - 1],
            color: params.color_rgba,
            stroke_width: params.stroke_width,
        })),
        ToolbarItem::ToolPen => Some(AnnotationShape::Pen(PenAction {
            points: pts.to_vec(),
            color: params.color_rgba,
            stroke_width: params.stroke_width,
        })),
        ToolbarItem::ToolMarker => Some(AnnotationShape::Marker(MarkerAction {
            points: pts.to_vec(),
            color: (
                params.color_rgba.0,
                params.color_rgba.1,
                params.color_rgba.2,
            ),
            stroke_width: params.stroke_width * 2.0,
        })),
        ToolbarItem::ToolMosaic => {
            let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
            Some(AnnotationShape::Mosaic(MosaicAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                cell_size: 10,
            }))
        }
        ToolbarItem::ToolBadge => {
            let center = pts[pts.len() - 1];
            let radius = (params.stroke_width * 3.0).clamp(10.0, 48.0) * params.ui_scale;
            Some(AnnotationShape::Badge(BadgeAction {
                center,
                radius,
                number: params.badge_number as usize,
                color: params.color_rgba,
            }))
        }
        _ => None,
    }
}

/// 将当前暂存的笔触或图形提交为持久化标注图元
pub fn commit_annotation(state: &mut SnipperState) {
    if let Some(params) = AnnotationDraftParams::from_state(state) {
        if let Some(shape) = build_shape(&params) {
            if params.tool == ToolbarItem::ToolBadge {
                state.next_badge_number += 1;
            }
            state.annotations.push(shape);
        }
    }
    state.pending_stroke_points.clear();
}
