//! 工具栏矢量图标绘制模块与分发调度。

pub mod actions;
pub mod primitives;
pub mod tools;

pub(crate) use primitives::{add_round_rect, make_paint, RoundRect};

use super::items::{Tool, ToolbarAction, ToolbarItem};
use crate::geometry::Rect;
use tiny_skia::{Color, PixmapMut, Stroke};

/// 工具栏图标的几何、状态与缩放参数。
pub struct IconRenderSpec<'a> {
    pub item: ToolbarItem,
    pub rect: &'a Rect,
    pub scale: f32,
    pub active_color: Color,
}

/// 绘制各项具体矢量图标 (严格对齐经典工具栏图标造型，深炭黑灰线条)
pub fn render_item_icon(pixmap: &mut PixmapMut, spec: IconRenderSpec<'_>) {
    let painter = IconPainter::new(spec.rect, spec.scale);
    match spec.item {
        ToolbarItem::Tool(tool) => render_tool_icon(pixmap, tool, spec.active_color, &painter),
        ToolbarItem::Action(action) => render_action_icon(pixmap, action, &painter),
    }
}

/// 单次图标绘制共享的中心点与画笔样式。
struct IconPainter {
    center_x: f32,
    center_y: f32,
    scale: f32,
    paint: tiny_skia::Paint<'static>,
    stroke: Stroke,
}

impl IconPainter {
    fn new(rect: &Rect, scale: f32) -> Self {
        Self {
            center_x: rect.x + rect.width / 2.0,
            center_y: rect.y + rect.height / 2.0,
            scale,
            paint: make_paint(Color::from_rgba8(45, 48, 56, 255)),
            stroke: Stroke {
                width: 1.8 * scale,
                line_cap: tiny_skia::LineCap::Round,
                line_join: tiny_skia::LineJoin::Round,
                ..Default::default()
            },
        }
    }
}

/// 按标注工具分派图标。
fn render_tool_icon(
    pixmap: &mut PixmapMut,
    tool: Tool,
    active_color: Color,
    painter: &IconPainter,
) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    match tool {
        Tool::Rect => tools::render_icon_rect(pixmap, cx, cy, s, paint, stroke),
        Tool::Ellipse => tools::render_icon_ellipse(pixmap, cx, cy, s, paint, stroke),
        Tool::Arrow => tools::render_icon_arrow(pixmap, cx, cy, s, active_color, paint, stroke),
        Tool::Pen => tools::render_icon_pen(pixmap, cx, cy, s, paint, stroke),
        Tool::Line => tools::render_icon_line(pixmap, cx, cy, s, paint, stroke),
        Tool::Eraser => tools::render_icon_eraser(pixmap, cx, cy, s, paint, stroke),
        Tool::Marker => tools::render_icon_marker(pixmap, cx, cy, s, paint, stroke),
        Tool::Mosaic => tools::render_icon_mosaic(pixmap, cx, cy, s, paint, stroke),
        Tool::Blur => tools::render_icon_blur(pixmap, cx, cy, s, paint, stroke),
        Tool::Spotlight => tools::render_icon_spotlight(pixmap, cx, cy, s, paint, stroke),
        Tool::Magnifier => tools::render_icon_magnifier(pixmap, cx, cy, s, paint, stroke),
        Tool::Text => tools::render_icon_text(pixmap, cx, cy, s, paint, stroke),
        Tool::Badge => tools::render_icon_badge(pixmap, cx, cy, s, paint, stroke),
    }
}

/// 按工具栏动作分派图标。
fn render_action_icon(pixmap: &mut PixmapMut, action: ToolbarAction, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    match action {
        ToolbarAction::Undo => actions::render_icon_undo(pixmap, cx, cy, s, paint, stroke),
        ToolbarAction::Redo => actions::render_icon_redo(pixmap, cx, cy, s, paint, stroke),
        ToolbarAction::Close => actions::render_icon_close(pixmap, cx, cy, s, paint, stroke),
        ToolbarAction::Pin => actions::render_icon_pin(pixmap, cx, cy, s, paint, stroke),
        ToolbarAction::Save => actions::render_icon_save(pixmap, cx, cy, s, paint, stroke),
        ToolbarAction::Copy => actions::render_icon_copy(pixmap, cx, cy, s, paint, stroke),
        ToolbarAction::Ocr => actions::render_icon_ocr(pixmap, cx, cy, s, paint, stroke),
        ToolbarAction::Qr => actions::render_icon_qr(pixmap, cx, cy, s, paint, stroke),
        ToolbarAction::SmartRedact => tools::render_icon_mosaic(pixmap, cx, cy, s, paint, stroke),
        ToolbarAction::ClearAnnotations => {
            actions::render_icon_close(pixmap, cx, cy, s, paint, stroke)
        }
        ToolbarAction::Translate => {
            actions::render_icon_translate(pixmap, cx, cy, s, paint, stroke)
        }
        ToolbarAction::MouseThrough => {
            actions::render_icon_mouse_through(pixmap, cx, cy, s, paint, stroke)
        }
    }
}
