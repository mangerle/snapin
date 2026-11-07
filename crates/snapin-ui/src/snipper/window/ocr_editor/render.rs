//! 文字识别全文编辑框渲染：外框、可拖拽标题栏、折行正文、选区、光标、滚动条与底部按钮。

use super::layout::{OcrEditorButton, OcrEditorLayout};
use super::state::OcrTextEditorState;
use crate::drawing::{add_round_rect, make_paint, RoundRect};
use crate::geometry::Rect;
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions, TextMeasureOptions};
use tiny_skia::{Color, Pixmap, PixmapMut, PixmapPaint, Stroke, Transform};

/// 绘制文字识别全文编辑框全套 UI。
pub(crate) fn render_ocr_editor(
    state: &OcrTextEditorState,
    pixmap: &mut PixmapMut,
    layout: &OcrEditorLayout,
) {
    render_panel_surface(pixmap, layout);
    render_header(state, pixmap, layout);
    render_body(state, pixmap, layout);
    render_footer(state, pixmap, layout);
}

/// 绘制面板背景与边框。
fn render_panel_surface(pixmap: &mut PixmapMut, layout: &OcrEditorLayout) {
    let mut panel = tiny_skia::PathBuilder::new();
    add_round_rect(
        &mut panel,
        RoundRect {
            x: layout.panel.x,
            y: layout.panel.y,
            width: layout.panel.width,
            height: layout.panel.height,
            radius: 10.0 * layout.scale,
        },
    );
    let Some(path) = panel.finish() else {
        return;
    };

    // 底板浅灰底色
    pixmap.fill_path(
        &path,
        &make_paint(Color::from_rgba8(248, 250, 252, 252)),
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    // 边框描边
    pixmap.stroke_path(
        &path,
        &make_paint(Color::from_rgba8(148, 163, 184, 240)),
        &Stroke {
            width: layout.scale,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
}

/// 绘制标题栏与右上角关闭按钮。
fn render_header(state: &OcrTextEditorState, pixmap: &mut PixmapMut, layout: &OcrEditorLayout) {
    let scale = layout.scale;
    with_thread_local_text_renderer(|renderer| {
        // 主标题
        renderer.draw_text_rgba(
            pixmap,
            snapin_foundation::i18n::tr("编辑识别文字"),
            TextDrawOptions::new(
                layout.header.x + 16.0 * scale,
                layout.header.y + 13.0 * scale,
                15.0 * scale,
                (15, 23, 42, 255),
            ),
        );
    });

    // 右上角关闭按钮
    let is_hovered = state.hovered_button == Some(OcrEditorButton::CloseHeader);
    let is_pressed = state.pressed_button == Some(OcrEditorButton::CloseHeader);
    if is_hovered || is_pressed {
        let bg_color = if is_pressed {
            Color::from_rgba8(226, 232, 240, 240)
        } else {
            Color::from_rgba8(241, 245, 249, 240)
        };
        let mut btn_path = tiny_skia::PathBuilder::new();
        add_round_rect(
            &mut btn_path,
            RoundRect {
                x: layout.close_button.x,
                y: layout.close_button.y,
                width: layout.close_button.width,
                height: layout.close_button.height,
                radius: 6.0 * scale,
            },
        );
        if let Some(path) = btn_path.finish() {
            pixmap.fill_path(
                &path,
                &make_paint(bg_color),
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    with_thread_local_text_renderer(|renderer| {
        let text_color = if is_hovered {
            (239, 68, 68, 255)
        } else {
            (100, 116, 139, 255)
        };
        renderer.draw_text_rgba(
            pixmap,
            "×",
            TextDrawOptions::new(
                layout.close_button.x + 8.0 * scale,
                layout.close_button.y + 4.0 * scale,
                16.0 * scale,
                text_color,
            ),
        );
    });
}

/// 绘制正文区域：独立位图裁剪、折行文本、选区高亮、光标及右侧滚动条。
fn render_body(state: &OcrTextEditorState, pixmap: &mut PixmapMut, layout: &OcrEditorLayout) {
    let body_width = layout.body.width.max(1.0) as u32;
    let body_height = layout.body.height.max(1.0) as u32;
    let Some(mut body) = Pixmap::new(body_width, body_height) else {
        return;
    };
    body.fill(Color::from_rgba8(255, 255, 255, 255));

    let scale = layout.scale;
    let font_size = 14.0 * scale;
    let line_height = 22.0 * scale;
    let padding_x = 12.0 * scale;
    let padding_y = 10.0 * scale;
    let max_text_width = (layout.body.width - padding_x * 2.0 - 12.0 * scale).max(10.0);

    let display_text = state.display_text();
    let measure_options = TextMeasureOptions::new(font_size, line_height, max_text_width, 10000.0);

    with_thread_local_text_renderer(|renderer| {
        // 1. 选区高亮绘制
        if let Some((sel_start, sel_end)) = state.selection_range() {
            let rects = renderer.measure_selection_rects(
                &display_text,
                sel_start,
                sel_end,
                measure_options,
            );
            let sel_color = Color::from_rgba8(186, 230, 253, 180);
            for r in rects {
                let rect_top = padding_y + r.y - state.scroll_y;
                let rect_bottom = rect_top + r.height;
                if rect_bottom < 0.0 || rect_top > layout.body.height {
                    continue;
                }
                let mut sel_path = tiny_skia::PathBuilder::new();
                crate::drawing::add_rect(
                    &mut sel_path,
                    padding_x + r.x,
                    rect_top,
                    r.width.max(2.0),
                    r.height,
                );
                if let Some(path) = sel_path.finish() {
                    body.as_mut().fill_path(
                        &path,
                        &make_paint(sel_color),
                        tiny_skia::FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }

        // 2. 正文折行文字绘制
        renderer.draw_multiline_text_rgba(
            &mut body.as_mut(),
            &display_text,
            padding_x,
            padding_y - state.scroll_y,
            measure_options,
            (30, 41, 59, 255),
        );

        // 3. 光标竖线绘制
        if state.is_cursor_blink_visible() {
            let metrics =
                renderer.measure_text_cursor(&display_text, state.cursor, measure_options);
            let cursor_x = padding_x + metrics.cursor_x;
            let cursor_y = padding_y + metrics.cursor_y - state.scroll_y;
            if cursor_y + line_height >= 0.0 && cursor_y <= layout.body.height {
                let mut cursor_path = tiny_skia::PathBuilder::new();
                crate::drawing::add_rect(
                    &mut cursor_path,
                    cursor_x,
                    cursor_y + 2.0 * scale,
                    1.5 * scale,
                    line_height - 4.0 * scale,
                );
                if let Some(path) = cursor_path.finish() {
                    body.as_mut().fill_path(
                        &path,
                        &make_paint(Color::from_rgba8(37, 99, 235, 255)),
                        tiny_skia::FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
    });

    // 4. 滚动条绘制
    if let (Some(track), Some(thumb)) = (layout.scrollbar_track, layout.scrollbar_thumb) {
        let rel_track_x = track.x - layout.body.x;
        let rel_track_y = track.y - layout.body.y;
        let rel_thumb_x = thumb.x - layout.body.x;
        let rel_thumb_y = thumb.y - layout.body.y;

        // 轨道
        let mut track_path = tiny_skia::PathBuilder::new();
        add_round_rect(
            &mut track_path,
            RoundRect {
                x: rel_track_x,
                y: rel_track_y,
                width: track.width,
                height: track.height,
                radius: track.width / 2.0,
            },
        );
        if let Some(path) = track_path.finish() {
            body.as_mut().fill_path(
                &path,
                &make_paint(Color::from_rgba8(241, 245, 249, 180)),
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        // 滑块
        let thumb_color = if state.is_dragging_scrollbar {
            Color::from_rgba8(100, 116, 139, 240)
        } else {
            Color::from_rgba8(148, 163, 184, 200)
        };
        let mut thumb_path = tiny_skia::PathBuilder::new();
        add_round_rect(
            &mut thumb_path,
            RoundRect {
                x: rel_thumb_x,
                y: rel_thumb_y,
                width: thumb.width,
                height: thumb.height,
                radius: thumb.width / 2.0,
            },
        );
        if let Some(path) = thumb_path.finish() {
            body.as_mut().fill_path(
                &path,
                &make_paint(thumb_color),
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    // 将独立位图绘制回主画面
    pixmap.draw_pixmap(
        layout.body.x as i32,
        layout.body.y as i32,
        body.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );

    // 绘制正文内框微细描边
    let mut border_path = tiny_skia::PathBuilder::new();
    add_round_rect(
        &mut border_path,
        RoundRect {
            x: layout.body.x,
            y: layout.body.y,
            width: layout.body.width,
            height: layout.body.height,
            radius: 4.0 * scale,
        },
    );
    if let Some(path) = border_path.finish() {
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(226, 232, 240, 255)),
            &Stroke {
                width: scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// 绘制底部字数统计与操作实体按钮组。
fn render_footer(state: &OcrTextEditorState, pixmap: &mut PixmapMut, layout: &OcrEditorLayout) {
    let scale = layout.scale;

    // 左侧字数与行数统计
    let count_text = format!("共 {} 字 · {} 行", state.char_count(), state.line_count());
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &count_text,
            TextDrawOptions::new(
                layout.count_badge.x + 4.0 * scale,
                layout.count_badge.y + 4.0 * scale,
                12.0 * scale,
                (100, 116, 139, 255),
            ),
        );
    });

    // 右侧各实体操作按钮
    for (btn, rect) in &layout.buttons {
        let is_hovered = state.hovered_button == Some(*btn);
        let is_pressed = state.pressed_button == Some(*btn);
        render_single_button(pixmap, *btn, *rect, scale, is_hovered, is_pressed);
    }
}

/// 绘制单个圆角实体按钮。
fn render_single_button(
    pixmap: &mut PixmapMut,
    btn: OcrEditorButton,
    rect: Rect,
    scale: f32,
    is_hovered: bool,
    is_pressed: bool,
) {
    let (bg_color, border_color, text_color) = if btn.is_primary() {
        if is_pressed {
            (
                Color::from_rgba8(30, 64, 175, 255),
                Color::from_rgba8(30, 64, 175, 255),
                (255, 255, 255, 255),
            )
        } else if is_hovered {
            (
                Color::from_rgba8(29, 78, 216, 255),
                Color::from_rgba8(29, 78, 216, 255),
                (255, 255, 255, 255),
            )
        } else {
            (
                Color::from_rgba8(37, 99, 235, 255),
                Color::from_rgba8(37, 99, 235, 255),
                (255, 255, 255, 255),
            )
        }
    } else if is_pressed {
        (
            Color::from_rgba8(226, 232, 240, 255),
            Color::from_rgba8(148, 163, 184, 255),
            (15, 23, 42, 255),
        )
    } else if is_hovered {
        (
            Color::from_rgba8(241, 245, 249, 255),
            Color::from_rgba8(203, 213, 225, 255),
            (15, 23, 42, 255),
        )
    } else {
        (
            Color::from_rgba8(255, 255, 255, 255),
            Color::from_rgba8(203, 213, 225, 255),
            (51, 65, 85, 255),
        )
    };

    let mut path_builder = tiny_skia::PathBuilder::new();
    add_round_rect(
        &mut path_builder,
        RoundRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            radius: 5.0 * scale,
        },
    );
    let Some(path) = path_builder.finish() else {
        return;
    };

    pixmap.fill_path(
        &path,
        &make_paint(bg_color),
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    pixmap.stroke_path(
        &path,
        &make_paint(border_color),
        &Stroke {
            width: scale,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );

    let label = snapin_foundation::i18n::tr(btn.label());
    with_thread_local_text_renderer(|renderer| {
        let metrics = renderer.measure_text_input(
            label,
            TextMeasureOptions::new(12.0 * scale, 14.0 * scale, rect.width, rect.height),
        );
        let text_x = rect.x + (rect.width - metrics.max_line_width) / 2.0;
        let text_y = rect.y + (rect.height - metrics.total_height) / 2.0;
        renderer.draw_text_rgba(
            pixmap,
            label,
            TextDrawOptions::new(text_x, text_y, 12.0 * scale, text_color),
        );
    });
}
