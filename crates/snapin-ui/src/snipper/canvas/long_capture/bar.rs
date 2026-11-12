//! 长截图控制栏尺寸信息、分组按钮与右侧拖动手柄的绘制辅助。

use super::icons::{
    draw_auto_scroll_icon, draw_bidirectional_arrow, draw_grip_dots, AutoScrollIconStyle,
};
use super::types;
use crate::drawing::{add_round_rect, RoundRect};
use crate::geometry::Rect;
use snapin_editor::toolbar::icons::{render_item_icon, IconRenderSpec};
use snapin_editor::toolbar::items::{ToolbarAction as EditorToolbarAction, ToolbarItem};
use snapin_render::stitching::StitchDirection;
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions};
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 尺寸信息区域的布局与内容参数。
pub(super) struct SizeReadoutSpec {
    pub(super) direction: StitchDirection,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) selection_w: u32,
    pub(super) selection_h: u32,
    pub(super) scale: f32,
}

/// 计算全部可交互按钮及按钮间距的总宽度。
pub(super) fn action_buttons_width(defs: &[types::LongCaptureButtonDef], s: f32) -> f32 {
    let button_width: f32 = defs.iter().map(|def| def.width * s).sum();
    let spacing = defs.len().saturating_sub(1) as f32 * 3.0 * s;
    button_width + spacing
}

/// 绘制尺寸与滚动轴信息。该区域仅用于展示，不响应拖动或点击。
pub(super) fn draw_size_readout(pixmap: &mut PixmapMut, spec: SizeReadoutSpec) {
    let size_text = format!("{}×{}", spec.selection_w, spec.selection_h);
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &size_text,
            TextDrawOptions::new(
                spec.x + 5.0 * spec.scale,
                spec.y + 5.0 * spec.scale,
                11.0 * spec.scale,
                (45, 52, 65, 255),
            ),
        );
    });
    draw_bidirectional_arrow(
        pixmap,
        spec.x + spec.width - 10.0 * spec.scale,
        spec.y + spec.height / 2.0,
        spec.direction,
        spec.scale,
    );
}

/// 绘制控制栏内的浅灰竖向分隔线。
pub(super) fn draw_vertical_divider(pixmap: &mut PixmapMut, x: f32, y: f32, height: f32, s: f32) {
    let mut path = PathBuilder::new();
    path.move_to(x, y + 4.0 * s);
    path.line_to(x, y + height - 4.0 * s);
    if let Some(path) = path.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 0, 22);
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 复用编辑器主工具栏的同一套动作图标，避免相同功能出现不同视觉语义。
pub(super) fn draw_editor_action_icon(
    pixmap: &mut PixmapMut,
    action: EditorToolbarAction,
    rect: &Rect,
    s: f32,
) {
    render_item_icon(
        pixmap,
        IconRenderSpec {
            item: ToolbarItem::Action(action),
            rect,
            scale: s,
            active_color: Color::from_rgba8(45, 48, 56, 255),
        },
    );
}

/// 绘制右端六点拖动手柄，与常规截图主工具栏保持相同抓握样式。
pub(super) fn draw_drag_handle(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
    let start_x = rect.x + (rect.width - 3.5 * s) / 2.0;
    let start_y = rect.y + (rect.height - 9.0 * s) / 2.0;
    draw_grip_dots(pixmap, start_x, start_y, s);
}

/// 绘制按钮浅灰悬停底板。
pub(super) fn draw_button_hover_bg(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
    draw_button_bg(pixmap, rect, s, 14);
}

/// 绘制按钮激活高亮底板。
pub(super) fn draw_button_active_bg(pixmap: &mut PixmapMut, rect: &Rect, s: f32) {
    draw_button_bg(pixmap, rect, s, 22);
}

/// 绘制统一圆角按钮底色。
fn draw_button_bg(pixmap: &mut PixmapMut, rect: &Rect, s: f32, alpha: u8) {
    draw_button_bg_rgba(pixmap, rect, s, (0, 0, 0, alpha));
}

fn draw_button_bg_rgba(pixmap: &mut PixmapMut, rect: &Rect, s: f32, rgba: (u8, u8, u8, u8)) {
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            radius: 4.0 * s,
        },
    );
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(rgba.0, rgba.1, rgba.2, rgba.3);
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制自动滚到底开关按钮及其激活/悬停状态。
pub(super) fn draw_auto_scroll_button(
    pixmap: &mut PixmapMut,
    rect: &Rect,
    direction: StitchDirection,
    is_active: bool,
    is_hovered: bool,
    s: f32,
) {
    if is_active {
        draw_button_bg_rgba(pixmap, rect, s, (37, 99, 235, 28));
    } else if is_hovered {
        draw_button_hover_bg(pixmap, rect, s);
    }
    draw_auto_scroll_icon(
        pixmap,
        rect.x + rect.width / 2.0,
        rect.y + rect.height / 2.0,
        AutoScrollIconStyle {
            scale: s,
            hovered: is_hovered,
            active: is_active,
            direction,
        },
    );
}
