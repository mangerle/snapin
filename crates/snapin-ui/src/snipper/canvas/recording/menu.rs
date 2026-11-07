//! 录制设置菜单与悬浮提示绘制。

use super::render::draw_panel;
use crate::drawing::{add_round_rect, make_paint, RoundRect};
use crate::geometry::Rect;
use crate::ports::RecordingFormat;
use crate::snipper::window::state::{RecordingItem, RecordingSession};
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions};
use tiny_skia::{Color, FillRule, PathBuilder, PixmapMut, Transform};

pub(super) struct SettingsMenuContext<'a> {
    pub(super) session: &'a RecordingSession,
    pub(super) rect: &'a Rect,
    pub(super) rows: &'a [(RecordingFormat, Rect)],
    pub(super) scale: f32,
}

pub(super) fn draw_settings_menu(pixmap: &mut PixmapMut, context: SettingsMenuContext<'_>) {
    let SettingsMenuContext {
        session,
        rect,
        rows,
        scale,
    } = context;
    draw_panel(pixmap, rect, scale);
    for (format, row) in rows {
        if session.format() == *format {
            let mut path = PathBuilder::new();
            add_round_rect(
                &mut path,
                RoundRect {
                    x: row.x,
                    y: row.y + 2.0 * scale,
                    width: row.width,
                    height: row.height - 4.0 * scale,
                    radius: 5.0 * scale,
                },
            );
            if let Some(path) = path.finish() {
                pixmap.fill_path(
                    &path,
                    &make_paint(Color::from_rgba8(37, 99, 235, 22)),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        with_thread_local_text_renderer(|renderer| {
            renderer.draw_text_rgba(
                pixmap,
                format.label(),
                TextDrawOptions::new(
                    row.x + 12.0 * scale,
                    row.y + 8.0 * scale,
                    12.0 * scale,
                    (31, 41, 55, 255),
                ),
            );
        });
    }
}

pub(super) fn draw_tooltip(
    pixmap: &mut PixmapMut,
    session: &RecordingSession,
    bar: &Rect,
    scale: f32,
) {
    let Some(item) = session.hovered() else {
        return;
    };
    let label = match item {
        RecordingItem::Tool(tool) => tool.label(),
        RecordingItem::Action(action) => action.label(),
    };
    let width = (label.chars().count() as f32 * 13.0 + 20.0).max(54.0) * scale;
    let rect = Rect {
        x: (bar.x + bar.width / 2.0 - width / 2.0).max(8.0),
        y: (bar.y - 34.0 * scale).max(8.0),
        width,
        height: 28.0 * scale,
    };
    draw_panel(pixmap, &rect, scale);
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            snapin_foundation::i18n::tr(label),
            TextDrawOptions::new(
                rect.x + 10.0 * scale,
                rect.y + 7.0 * scale,
                11.0 * scale,
                (31, 41, 55, 255),
            ),
        );
    });
}
