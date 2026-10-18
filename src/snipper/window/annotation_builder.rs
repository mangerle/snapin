//! 标注动作图元工厂与提交逻辑实现。

use super::events::SnipperState;
use crate::annotation::shapes::{
    ArrowAction, BadgeAction, EllipseAction, MarkerAction, MosaicAction, PenAction, RectAction,
};
use crate::snipper::selection::Rect;
use crate::snipper::toolbar::ToolbarItem;

/// 将当前暂存的笔触或图形提交为持久化标注图元
pub fn commit_annotation(state: &mut SnipperState) {
    let Some(tool) = state.toolbar.active_tool else {
        return;
    };
    let pts = &state.pending_stroke_points;
    if pts.is_empty() {
        return;
    }
    if tool != ToolbarItem::ToolBadge && pts.len() < 2 {
        state.pending_stroke_points.clear();
        return;
    }

    let stroke_w = state.toolbar.sub_options.current_stroke_width();
    let c = state.toolbar.sub_options.color;
    let rgba = (
        (c.red() * 255.0).round() as u8,
        (c.green() * 255.0).round() as u8,
        (c.blue() * 255.0).round() as u8,
        (c.alpha() * 255.0).round() as u8,
    );

    match tool {
        ToolbarItem::ToolRect => {
            let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
            state.annotations.push(Box::new(RectAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                color: rgba,
                stroke_width: stroke_w,
                is_filled: false,
            }));
        }
        ToolbarItem::ToolEllipse => {
            let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
            state.annotations.push(Box::new(EllipseAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                color: rgba,
                stroke_width: stroke_w,
                is_filled: false,
            }));
        }
        ToolbarItem::ToolBadge => {
            let center = pts[pts.len() - 1];
            let number = state.next_badge_number;
            state.next_badge_number += 1;
            let radius = (stroke_w * 3.0).clamp(10.0, 48.0) * state.toolbar.ui_scale;
            state.annotations.push(Box::new(BadgeAction {
                center,
                radius,
                number: number as usize,
                color: rgba,
            }));
        }
        ToolbarItem::ToolArrow => {
            state.annotations.push(Box::new(ArrowAction {
                start: pts[0],
                end: pts[pts.len() - 1],
                color: rgba,
                stroke_width: stroke_w,
            }));
        }
        ToolbarItem::ToolPen => {
            state.annotations.push(Box::new(PenAction {
                points: pts.clone(),
                color: rgba,
                stroke_width: stroke_w,
            }));
        }
        ToolbarItem::ToolMarker => {
            state.annotations.push(Box::new(MarkerAction {
                points: pts.clone(),
                color: (rgba.0, rgba.1, rgba.2),
                stroke_width: stroke_w * 2.0,
            }));
        }
        ToolbarItem::ToolMosaic => {
            let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
            state.annotations.push(Box::new(MosaicAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                cell_size: 10,
            }));
        }
        _ => {}
    }

    state.pending_stroke_points.clear();
}
