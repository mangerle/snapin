//! 录屏结果工具栏的播放、打开文件与复制路径图标。

use super::{active, draw_hover, ink, paint, stroke, Visual};
use tiny_skia::{FillRule, PathBuilder, PixmapMut, Transform};

/// 绘制内嵌预览的播放或暂停按钮。
pub(super) fn draw_playback(pixmap: &mut PixmapMut, visual: &Visual<'_>, playing: bool) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered {
        draw_hover(pixmap, rect, scale);
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    if !playing {
        let mut path = PathBuilder::new();
        path.move_to(cx - 5.0 * scale, cy - 6.0 * scale);
        path.line_to(cx - 5.0 * scale, cy + 6.0 * scale);
        path.line_to(cx + 6.0 * scale, cy);
        path.close();
        if let Some(path) = path.finish() {
            pixmap.fill_path(
                &path,
                &paint(ink()),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    } else {
        let mut path = PathBuilder::new();
        for rect in [
            tiny_skia::Rect::from_xywh(
                cx - 5.0 * scale,
                cy - 6.0 * scale,
                3.5 * scale,
                12.0 * scale,
            ),
            tiny_skia::Rect::from_xywh(
                cx + 1.5 * scale,
                cy - 6.0 * scale,
                3.5 * scale,
                12.0 * scale,
            ),
        ]
        .into_iter()
        .flatten()
        {
            path.push_rect(rect);
        }
        if let Some(path) = path.finish() {
            pixmap.fill_path(
                &path,
                &paint(ink()),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

/// 绘制打开文件的文件夹与向上箭头。
pub(super) fn draw_open_file(pixmap: &mut PixmapMut, visual: &Visual<'_>) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered {
        draw_hover(pixmap, rect, scale);
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut path = PathBuilder::new();
    path.move_to(cx - 7.0 * scale, cy - 2.0 * scale);
    path.line_to(cx - 3.0 * scale, cy - 2.0 * scale);
    path.line_to(cx - 1.5 * scale, cy - 4.0 * scale);
    path.line_to(cx + 4.0 * scale, cy - 4.0 * scale);
    path.line_to(cx + 7.0 * scale, cy - 1.0 * scale);
    path.line_to(cx + 7.0 * scale, cy + 5.0 * scale);
    path.line_to(cx - 7.0 * scale, cy + 5.0 * scale);
    path.close();
    if let Some(path) = path.finish() {
        pixmap.stroke_path(
            &path,
            &paint(ink()),
            &stroke(scale),
            Transform::identity(),
            None,
        );
    }
    let mut arrow = PathBuilder::new();
    arrow.move_to(cx, cy + 3.0 * scale);
    arrow.line_to(cx, cy - 1.0 * scale);
    arrow.move_to(cx - 2.2 * scale, cy + 1.0 * scale);
    arrow.line_to(cx, cy - 1.0 * scale);
    arrow.line_to(cx + 2.2 * scale, cy + 1.0 * scale);
    if let Some(path) = arrow.finish() {
        pixmap.stroke_path(
            &path,
            &paint(active()),
            &stroke(scale),
            Transform::identity(),
            None,
        );
    }
}

/// 绘制复制文件路径的双层文档图标。
pub(super) fn draw_copy_path(pixmap: &mut PixmapMut, visual: &Visual<'_>) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered {
        draw_hover(pixmap, rect, scale);
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    for (offset_x, offset_y) in [(-3.0_f32, -4.0_f32), (0.0, -1.0)] {
        let mut path = PathBuilder::new();
        path.move_to(cx + (offset_x - 3.0) * scale, cy + (offset_y + 5.0) * scale);
        path.line_to(cx + (offset_x - 3.0) * scale, cy + offset_y * scale);
        path.line_to(cx + (offset_x + 5.0) * scale, cy + offset_y * scale);
        path.line_to(cx + (offset_x + 5.0) * scale, cy + (offset_y + 5.0) * scale);
        if let Some(path) = path.finish() {
            pixmap.stroke_path(
                &path,
                &paint(ink()),
                &stroke(scale),
                Transform::identity(),
                None,
            );
        }
    }
}
