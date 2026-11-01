//! 工具栏矢量图标绘制模块与分发调度。

pub mod actions;
pub mod primitives;
pub mod tools;

pub(crate) use primitives::{add_round_rect, make_paint};

use super::items::{Tool, ToolbarAction, ToolbarItem};
use crate::geometry::Rect;
use tiny_skia::{Color, PixmapMut, Stroke};

/// 绘制各项具体矢量图标 (严格对齐经典工具栏图标造型，深炭黑灰线条)
pub fn render_item_icon(
    pixmap: &mut PixmapMut,
    item: ToolbarItem,
    rect: &Rect,
    s: f32,
    active_color: Color,
) {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let paint = make_paint(Color::from_rgba8(45, 48, 56, 255));
    let stroke = Stroke {
        width: 1.8 * s,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    };

    match item {
        ToolbarItem::Tool(Tool::Rect) => {
            tools::render_icon_rect(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Tool(Tool::Arrow) => {
            tools::render_icon_arrow(pixmap, cx, cy, s, active_color, &paint, &stroke)
        }
        ToolbarItem::Tool(Tool::Pen) => tools::render_icon_pen(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::Tool(Tool::Marker) => {
            tools::render_icon_marker(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Tool(Tool::Mosaic) => {
            tools::render_icon_mosaic(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Tool(Tool::Blur) => {
            tools::render_icon_blur(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Tool(Tool::Spotlight) => {
            tools::render_icon_spotlight(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Tool(Tool::Text) => {
            tools::render_icon_text(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Tool(Tool::Badge) => {
            tools::render_icon_badge(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Action(ToolbarAction::Undo) => {
            actions::render_icon_undo(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Action(ToolbarAction::Redo) => {
            actions::render_icon_redo(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Action(ToolbarAction::Close) => {
            actions::render_icon_close(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Action(ToolbarAction::Pin) => {
            actions::render_icon_pin(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Action(ToolbarAction::Save) => {
            actions::render_icon_save(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Action(ToolbarAction::Copy) => {
            actions::render_icon_copy(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Action(ToolbarAction::Ocr) => {
            actions::render_icon_ocr(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Action(ToolbarAction::Translate) => {
            actions::render_icon_translate(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Action(ToolbarAction::MouseThrough) => {
            actions::render_icon_mouse_through(pixmap, cx, cy, s, &paint, &stroke)
        }
        ToolbarItem::Tool(Tool::Ellipse) => {
            tools::render_icon_ellipse(pixmap, cx, cy, s, &paint, &stroke)
        }
    }
}
