//! 工具栏矢量图标绘制模块与分发调度。

mod actions;
mod primitives;
mod tools;

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
    let painter = IconPainter::new(spec.rect, spec.scale, spec.active_color);
    match spec.item {
        ToolbarItem::Tool(tool) => render_tool_icon(pixmap, tool, &painter),
        ToolbarItem::Action(action) => render_action_icon(pixmap, action, &painter),
    }
}

/// 单次图标绘制共享的中心点与画笔样式。
struct IconPainter {
    center_x: f32,
    center_y: f32,
    scale: f32,
    active_color: Color,
    paint: tiny_skia::Paint<'static>,
    stroke: Stroke,
}

impl IconPainter {
    fn new(rect: &Rect, scale: f32, active_color: Color) -> Self {
        Self {
            center_x: rect.x + rect.width / 2.0,
            center_y: rect.y + rect.height / 2.0,
            scale,
            active_color,
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
fn render_tool_icon(pixmap: &mut PixmapMut, tool: Tool, painter: &IconPainter) {
    match tool {
        Tool::Rect => tools::render_icon_rect(pixmap, painter),
        Tool::Ellipse => tools::render_icon_ellipse(pixmap, painter),
        Tool::Arrow => tools::render_icon_arrow(pixmap, painter),
        Tool::Pen => tools::render_icon_pen(pixmap, painter),
        Tool::Line => tools::render_icon_line(pixmap, painter),
        Tool::Eraser => tools::render_icon_eraser(pixmap, painter),
        Tool::Marker => tools::render_icon_marker(pixmap, painter),
        Tool::Mosaic => tools::render_icon_mosaic(pixmap, painter),
        Tool::Blur => tools::render_icon_blur(pixmap, painter),
        Tool::Spotlight => tools::render_icon_spotlight(pixmap, painter),
        Tool::Magnifier => tools::render_icon_magnifier(pixmap, painter),
        Tool::Text => tools::render_icon_text(pixmap, painter),
        Tool::Badge => tools::render_icon_badge(pixmap, painter),
    }
}

/// 按工具栏动作分派图标。
fn render_action_icon(pixmap: &mut PixmapMut, action: ToolbarAction, painter: &IconPainter) {
    match action {
        ToolbarAction::Undo => actions::render_icon_undo(pixmap, painter),
        ToolbarAction::Redo => actions::render_icon_redo(pixmap, painter),
        ToolbarAction::Close => actions::render_icon_close(pixmap, painter),
        ToolbarAction::Pin => actions::render_icon_pin(pixmap, painter),
        ToolbarAction::Save => actions::render_icon_save(pixmap, painter),
        ToolbarAction::Copy => actions::render_icon_copy(pixmap, painter),
        ToolbarAction::Ocr => actions::render_icon_ocr(pixmap, painter),
        ToolbarAction::Qr => actions::render_icon_qr(pixmap, painter),
        ToolbarAction::SmartRedact => tools::render_icon_mosaic(pixmap, painter),
        ToolbarAction::ClearAnnotations => actions::render_icon_close(pixmap, painter),
        ToolbarAction::Translate => actions::render_icon_translate(pixmap, painter),
        ToolbarAction::MouseThrough => actions::render_icon_mouse_through(pixmap, painter),
    }
}
