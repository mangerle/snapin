//! 录制工具栏绘制实现。

use super::icons::*;
use super::layout::{calculate_recording_layout, RecordingLayoutRequest, RecordingSlotKind};
use super::menu::{draw_settings_menu, draw_tooltip, SettingsMenuContext};
use crate::drawing::{add_round_rect, make_paint, RoundRect};
use crate::editor::toolbar::Toolbar;
use crate::geometry::Rect;
use crate::snipper::window::state::{
    RecordingAction, RecordingItem, RecordingSession, RecordingStatus,
};
use snapin_editor::toolbar::icons::{render_item_icon, IconRenderSpec};
use snapin_editor::toolbar::{ToolbarAction, ToolbarItem};
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions, TextMeasureOptions};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, PixmapMut, Transform};

/// 录制工具栏完整渲染参数。
pub(crate) struct RecordingRenderContext<'a> {
    /// 当前录制会话。
    pub(crate) session: &'a RecordingSession,
    /// 标注工具栏状态。
    pub(crate) toolbar: &'a Toolbar,
    /// 当前选区。
    pub(crate) selection: &'a Rect,
    /// 覆盖层物理尺寸。
    pub(crate) screen_size: (u32, u32),
    /// 界面缩放。
    pub(crate) ui_scale: f32,
}

/// 绘制录制工具栏。
pub(crate) fn render_recording_toolbar(
    pixmap: &mut PixmapMut,
    context: RecordingRenderContext<'_>,
) {
    let RecordingRenderContext {
        session,
        toolbar,
        selection,
        screen_size,
        ui_scale,
    } = context;
    let layout = calculate_recording_layout(RecordingLayoutRequest {
        session,
        selection,
        screen_width: screen_size.0 as f32,
        screen_height: screen_size.1 as f32,
        scale: ui_scale,
    });
    let ui_scale = layout.scale;
    draw_panel(pixmap, &layout.bar_rect, ui_scale);

    for slot in &layout.slots {
        match slot.kind {
            RecordingSlotKind::Timer => draw_timer(pixmap, session, &slot.rect, ui_scale),
            RecordingSlotKind::Fps => draw_fps(pixmap, session, &slot.rect, ui_scale),
            RecordingSlotKind::Separator => draw_separator(pixmap, &slot.rect, ui_scale),
            RecordingSlotKind::DragHandle => {
                if session.hovered() == Some(RecordingItem::Action(RecordingAction::DragHandle)) {
                    draw_hover(pixmap, &slot.rect, ui_scale);
                }
                draw_drag_handle(pixmap, &slot.rect, ui_scale);
            }
            RecordingSlotKind::Chevron => draw_icon(
                pixmap,
                IconSpec {
                    rect: &slot.rect,
                    scale: ui_scale,
                    hovered: false,
                    variant: IconVariant::Chevron,
                },
            ),
            RecordingSlotKind::Item => {
                if let Some(item) = slot.item {
                    draw_item(
                        pixmap,
                        ItemDrawContext {
                            session,
                            toolbar,
                            item,
                            rect: &slot.rect,
                            scale: ui_scale,
                        },
                    );
                }
            }
        }
    }
    if let Some(rect) = layout.menu_rect {
        draw_settings_menu(
            pixmap,
            SettingsMenuContext {
                session,
                rect: &rect,
                rows: &layout.menu_rows,
                scale: ui_scale,
            },
        );
    }
    draw_tooltip(pixmap, session, &layout.bar_rect, ui_scale);
}

pub(super) fn draw_panel(pixmap: &mut PixmapMut, rect: &Rect, scale: f32) {
    let mut shadow = PathBuilder::new();
    add_round_rect(
        &mut shadow,
        RoundRect {
            x: rect.x,
            y: rect.y + 2.5 * scale,
            width: rect.width,
            height: rect.height,
            radius: 11.0 * scale,
        },
    );
    if let Some(path) = shadow.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(15, 23, 42, 34)),
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
            radius: 11.0 * scale,
        },
    );
    if let Some(path) = panel.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(255, 255, 255, 250)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        let mut border = Paint::default();
        border.set_color_rgba8(15, 23, 42, 22);
        pixmap.stroke_path(
            &path,
            &border,
            &tiny_skia::Stroke {
                width: 1.0 * scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

fn draw_separator(pixmap: &mut PixmapMut, rect: &Rect, scale: f32) {
    let x = rect.x + rect.width / 2.0;
    let mut path = PathBuilder::new();
    path.move_to(x, rect.y + 10.0 * scale);
    path.line_to(x, rect.y + rect.height - 10.0 * scale);
    if let Some(path) = path.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(15, 23, 42, 24);
        pixmap.stroke_path(
            &path,
            &paint,
            &tiny_skia::Stroke {
                width: 1.0 * scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

fn draw_timer(pixmap: &mut PixmapMut, session: &RecordingSession, rect: &Rect, scale: f32) {
    let text = format_elapsed(session.elapsed().as_secs());
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &text,
            TextDrawOptions::new(
                rect.x + 8.0 * scale,
                rect.y + 13.0 * scale,
                13.0 * scale,
                (24, 28, 36, 255),
            ),
        );
    });
}

fn draw_fps(pixmap: &mut PixmapMut, session: &RecordingSession, rect: &Rect, scale: f32) {
    if session.hovered() == Some(RecordingItem::Action(RecordingAction::CycleFps)) {
        draw_hover(pixmap, rect, scale);
    }
    let text = format!("{}FPS", session.fps());
    let font_size = 11.5 * scale;
    with_thread_local_text_renderer(|renderer| {
        let metrics = renderer.measure_text_input(
            &text,
            TextMeasureOptions::new(font_size, font_size * 1.25, rect.width, rect.height),
        );
        let (x, y) = centered_text_origin(rect, metrics.max_line_width, metrics.total_height);
        renderer.draw_text_rgba(
            pixmap,
            &text,
            TextDrawOptions::new(x, y, font_size, (75, 85, 99, 255)),
        );
    });
}

/// 计算单行文字在矩形内的居中绘制起点。
fn centered_text_origin(rect: &Rect, text_width: f32, text_height: f32) -> (f32, f32) {
    (
        rect.x + (rect.width - text_width).max(0.0) / 2.0,
        rect.y + (rect.height - text_height).max(0.0) / 2.0,
    )
}

struct ItemDrawContext<'a> {
    session: &'a RecordingSession,
    toolbar: &'a Toolbar,
    item: RecordingItem,
    rect: &'a Rect,
    scale: f32,
}

fn draw_item(pixmap: &mut PixmapMut, context: ItemDrawContext<'_>) {
    let ItemDrawContext {
        session,
        toolbar,
        item,
        rect,
        scale,
    } = context;
    let hovered = session.hovered() == Some(item);
    match item {
        RecordingItem::Tool(tool) => {
            if hovered {
                draw_hover(pixmap, rect, scale);
            }
            let active = toolbar.active_tool == Some(tool);
            if active {
                draw_active(pixmap, rect, scale);
            }
            render_item_icon(
                pixmap,
                IconRenderSpec {
                    item: ToolbarItem::Tool(tool),
                    rect,
                    scale,
                    active_color: toolbar.sub_options.color,
                },
            );
        }
        RecordingItem::Action(action) => draw_action(
            pixmap,
            ActionDrawContext {
                session,
                action,
                rect,
                scale,
            },
        ),
    }
}

struct ActionDrawContext<'a> {
    session: &'a RecordingSession,
    action: RecordingAction,
    rect: &'a Rect,
    scale: f32,
}

fn draw_action(pixmap: &mut PixmapMut, context: ActionDrawContext<'_>) {
    let ActionDrawContext {
        session,
        action,
        rect,
        scale,
    } = context;
    let hovered = session.hovered() == Some(RecordingItem::Action(action));
    match action {
        RecordingAction::ToggleCapture => draw_icon(
            pixmap,
            IconSpec {
                rect,
                scale,
                hovered,
                variant: IconVariant::Record {
                    recording: session.status().is_recording(),
                },
            },
        ),
        RecordingAction::TogglePause => draw_icon(
            pixmap,
            IconSpec {
                rect,
                scale,
                hovered,
                variant: IconVariant::Pause {
                    paused: session.status() == RecordingStatus::Paused,
                    enabled: matches!(
                        session.status(),
                        RecordingStatus::Active | RecordingStatus::Paused
                    ),
                },
            },
        ),
        RecordingAction::ToggleAudio => draw_icon(
            pixmap,
            IconSpec {
                rect,
                scale,
                hovered,
                variant: IconVariant::Audio {
                    enabled: session.include_system_audio(),
                },
            },
        ),
        RecordingAction::ToggleCursor => draw_icon(
            pixmap,
            IconSpec {
                rect,
                scale,
                hovered,
                variant: IconVariant::Cursor {
                    enabled: session.include_cursor(),
                },
            },
        ),
        RecordingAction::ToggleSettings => draw_icon(
            pixmap,
            IconSpec {
                rect,
                scale,
                hovered,
                variant: IconVariant::Settings,
            },
        ),
        RecordingAction::CycleFps => {}
        RecordingAction::Undo => {
            if hovered {
                draw_hover(pixmap, rect, scale);
            }
            render_item_icon(
                pixmap,
                IconRenderSpec {
                    item: ToolbarItem::Action(ToolbarAction::Undo),
                    rect,
                    scale,
                    active_color: Color::BLACK,
                },
            );
        }
        RecordingAction::DragHandle => {}
        RecordingAction::Move => draw_icon(
            pixmap,
            IconSpec {
                rect,
                scale,
                hovered,
                variant: IconVariant::Move,
            },
        ),
        RecordingAction::Close => draw_icon(
            pixmap,
            IconSpec {
                rect,
                scale,
                hovered,
                variant: IconVariant::Close,
            },
        ),
        RecordingAction::Restart => draw_icon(
            pixmap,
            IconSpec {
                rect,
                scale,
                hovered,
                variant: IconVariant::Restart,
            },
        ),
    }
}

fn draw_hover(pixmap: &mut PixmapMut, rect: &Rect, scale: f32) {
    let mut path = PathBuilder::new();
    add_round_rect(
        &mut path,
        RoundRect {
            x: rect.x + 2.0 * scale,
            y: rect.y + 4.0 * scale,
            width: rect.width - 4.0 * scale,
            height: rect.height - 8.0 * scale,
            radius: 5.0 * scale,
        },
    );
    if let Some(path) = path.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(15, 23, 42, 18)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn draw_active(pixmap: &mut PixmapMut, rect: &Rect, scale: f32) {
    let mut path = PathBuilder::new();
    add_round_rect(
        &mut path,
        RoundRect {
            x: rect.x + 2.0 * scale,
            y: rect.y + 4.0 * scale,
            width: rect.width - 4.0 * scale,
            height: rect.height - 8.0 * scale,
            radius: 5.0 * scale,
        },
    );
    if let Some(path) = path.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(37, 99, 235, 24)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

pub(super) fn format_elapsed(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = seconds % 3600 / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centered_text_origin_centers_both_axes() {
        let rect = Rect {
            x: 100.0,
            y: 20.0,
            width: 80.0,
            height: 40.0,
        };
        assert_eq!(centered_text_origin(&rect, 48.0, 20.0), (116.0, 30.0));
    }
}
