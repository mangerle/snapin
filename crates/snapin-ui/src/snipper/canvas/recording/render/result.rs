//! 录屏结果页的文件摘要与回放进度绘制。

use super::format_elapsed;
use crate::drawing::{add_round_rect, make_paint, RoundRect};
use crate::geometry::Rect;
use crate::snipper::window::state::RecordingSession;
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions, TextMeasureOptions};
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, PixmapMut, Point,
    SpreadMode, Transform,
};

/// 绘制并反馈结果页回放进度。
pub(super) fn draw_preview_progress(
    pixmap: &mut PixmapMut,
    session: &RecordingSession,
    rect: &Rect,
    scale: f32,
) {
    let Some((elapsed, total)) = session.preview_elapsed() else {
        return;
    };
    let progress = session.preview_progress().clamp(0.0, 1.0);
    let radius = rect.height / 2.0;
    let seeking = session.is_seeking_preview();

    let mut shadow = PathBuilder::new();
    add_round_rect(
        &mut shadow,
        RoundRect {
            x: rect.x,
            y: rect.y + 3.0 * scale,
            width: rect.width,
            height: rect.height,
            radius,
        },
    );
    if let Some(path) = shadow.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(2, 6, 23, 48)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let mut panel = PathBuilder::new();
    add_round_rect(
        &mut panel,
        RoundRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            radius,
        },
    );
    if let Some(path) = panel.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(15, 23, 42, 218)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        let mut border = Paint::default();
        border.set_color_rgba8(255, 255, 255, if seeking { 92 } else { 34 });
        pixmap.stroke_path(
            &path,
            &border,
            &tiny_skia::Stroke {
                width: scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }

    let current_text = format_elapsed(elapsed.as_secs());
    let total_text = format_elapsed(total.as_secs());
    let font_size = 10.5 * scale;
    let track_left = rect.x + 72.0 * scale;
    let track_right = rect.x + rect.width - 72.0 * scale;
    let track_width = (track_right - track_left).max(1.0);
    let track_height = 4.0 * scale;
    let track_y = rect.y + (rect.height - track_height) / 2.0;

    let mut track = PathBuilder::new();
    add_round_rect(
        &mut track,
        RoundRect {
            x: track_left,
            y: track_y,
            width: track_width,
            height: track_height,
            radius: track_height / 2.0,
        },
    );
    if let Some(path) = track.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(255, 255, 255, 48)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let fill_width = (track_width * progress).max(if progress > 0.0 { 2.0 } else { 0.0 });
    if fill_width > 0.0 {
        let mut fill = PathBuilder::new();
        add_round_rect(
            &mut fill,
            RoundRect {
                x: track_left,
                y: track_y,
                width: fill_width,
                height: track_height,
                radius: (track_height / 2.0).min(fill_width / 2.0),
            },
        );
        if let Some(path) = fill.finish() {
            let mut paint = Paint::default();
            if let Some(shader) = LinearGradient::new(
                Point::from_xy(track_left, track_y),
                Point::from_xy(track_left + track_width, track_y),
                vec![
                    GradientStop::new(0.0, Color::from_rgba8(0, 194, 255, 238)),
                    GradientStop::new(1.0, Color::from_rgba8(37, 99, 235, 238)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            ) {
                paint.shader = shader;
            } else {
                paint.set_color_rgba8(0, 174, 255, 238);
            }
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    let cx = track_left + track_width * progress;
    let cy = rect.y + rect.height / 2.0;
    let mut glow = PathBuilder::new();
    glow.push_circle(cx, cy, if seeking { 8.0 * scale } else { 6.8 * scale });
    if let Some(path) = glow.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(
                0,
                194,
                255,
                if seeking { 90 } else { 52 },
            )),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let mut knob = PathBuilder::new();
    knob.push_circle(cx, cy, 4.4 * scale);
    if let Some(path) = knob.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(255, 255, 255, 250)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        let mut border = Paint::default();
        border.set_color_rgba8(0, 174, 255, 255);
        pixmap.stroke_path(
            &path,
            &border,
            &tiny_skia::Stroke {
                width: 1.4 * scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }

    with_thread_local_text_renderer(|renderer| {
        let text_y = rect.y + (rect.height - font_size * 1.25) / 2.0;
        renderer.draw_text_rgba(
            pixmap,
            &current_text,
            TextDrawOptions::new(
                rect.x + 14.0 * scale,
                text_y,
                font_size,
                (255, 255, 255, 238),
            ),
        );
        let total_metrics = renderer.measure_text_input(
            &total_text,
            TextMeasureOptions::new(font_size, font_size * 1.25, rect.width, rect.height),
        );
        let total_x = rect.x + rect.width - 14.0 * scale - total_metrics.max_line_width;
        renderer.draw_text_rgba(
            pixmap,
            &total_text,
            TextDrawOptions::new(total_x, text_y, font_size, (255, 255, 255, 156)),
        );
    });
}

pub(super) fn draw_result_file(
    pixmap: &mut PixmapMut,
    session: &RecordingSession,
    rect: &Rect,
    scale: f32,
) {
    let Some(artifact) = session.artifact() else {
        return;
    };
    let name = artifact
        .path
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_else(|| artifact.path.to_string_lossy());
    let mut chars = name.chars();
    let text: String = chars.by_ref().take(30).collect();
    let text = if chars.next().is_some() {
        format!("{text}...")
    } else {
        text
    };
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &text,
            TextDrawOptions::new(
                rect.x + 8.0 * scale,
                rect.y + 13.0 * scale,
                11.0 * scale,
                (55, 65, 81, 255),
            ),
        );
    });
}
