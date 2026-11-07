//! 录制工具栏的响应式布局与命中测试。

use crate::editor::toolbar::Tool;
use crate::geometry::Rect;
use crate::ports::RecordingFormat;
use crate::snipper::window::state::{RecordingAction, RecordingItem, RecordingSession};

/// 工具栏槽位的渲染类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecordingSlotKind {
    /// 录制时长文字。
    Timer,
    /// 帧率文字。
    Fps,
    /// 分组分隔线。
    Separator,
    /// 下拉箭头。
    Chevron,
    /// 右侧六点拖动手柄。
    DragHandle,
    /// 可交互按钮。
    Item,
}

/// 工具栏中的一个可渲染槽位。
#[derive(Debug, Clone)]
pub(crate) struct RecordingSlot {
    /// 槽位渲染类别。
    pub(crate) kind: RecordingSlotKind,
    /// 可交互项；仅 `Item` 类别存在。
    pub(crate) item: Option<RecordingItem>,
    /// 槽位屏幕矩形。
    pub(crate) rect: Rect,
}

/// 录制工具栏完整布局。
#[derive(Debug, Clone)]
pub(crate) struct RecordingLayout {
    /// 为适应当前屏幕宽度后实际采用的缩放。
    pub(crate) scale: f32,
    /// 白色工具栏底板。
    pub(crate) bar_rect: Rect,
    /// 按从左到右顺序排列的槽位。
    pub(crate) slots: Vec<RecordingSlot>,
    /// 设置菜单面板。
    pub(crate) menu_rect: Option<Rect>,
    /// 设置菜单中的格式选项。
    pub(crate) menu_rows: Vec<(RecordingFormat, Rect)>,
}

impl RecordingLayout {
    /// 命中工具栏项。
    pub(crate) fn hit(&self, x: f32, y: f32) -> Option<RecordingItem> {
        self.slots
            .iter()
            .find(|slot| slot.rect.contains(x, y))
            .and_then(|slot| slot.item)
    }

    /// 命中设置菜单中的格式选项。
    pub(crate) fn hit_menu(&self, x: f32, y: f32) -> Option<RecordingFormat> {
        self.menu_rows
            .iter()
            .find(|(_, rect)| rect.contains(x, y))
            .map(|(format, _)| *format)
    }

    /// 是否位于工具栏或展开菜单内。
    pub(crate) fn contains(&self, x: f32, y: f32) -> bool {
        self.bar_rect.contains(x, y) || self.menu_rect.is_some_and(|rect| rect.contains(x, y))
    }
}

/// 录制工具栏布局请求。
#[derive(Clone, Copy)]
pub(crate) struct RecordingLayoutRequest<'a> {
    /// 当前录制会话。
    pub(crate) session: &'a RecordingSession,
    /// 当前锁定选区。
    pub(crate) selection: &'a Rect,
    /// 覆盖层物理宽度。
    pub(crate) screen_width: f32,
    /// 覆盖层物理高度。
    pub(crate) screen_height: f32,
    /// 界面缩放。
    pub(crate) scale: f32,
}

/// 根据选区与屏幕边界计算录制工具栏布局。
pub(crate) fn calculate_recording_layout(request: RecordingLayoutRequest<'_>) -> RecordingLayout {
    let RecordingLayoutRequest {
        session,
        selection,
        screen_width,
        screen_height,
        scale,
    } = request;
    // 工具栏在 1 倍缩放时约 691 像素宽；除左右边距外还需预留缩放后的安全间隔，
    // 高 DPI 小屏上必须主动收缩，否则最右侧拖动手柄会被裁出屏幕。
    let scale = scale.min((screen_width / 720.0).max(0.8));
    let bar_height = 42.0 * scale;
    let mut slots = Vec::with_capacity(25);
    let mut cursor = LayoutCursor {
        x: 10.0 * scale,
        scale,
        bar_height,
    };
    push_special(
        &mut slots,
        &mut cursor,
        RecordingSlotKind::Timer,
        58.0 * scale,
    );
    push_special(
        &mut slots,
        &mut cursor,
        RecordingSlotKind::Fps,
        48.0 * scale,
    );
    if let Some(slot) = slots.last_mut() {
        slot.item = Some(RecordingItem::Action(RecordingAction::CycleFps));
    }
    for action in [
        RecordingAction::ToggleCapture,
        RecordingAction::TogglePause,
        RecordingAction::ToggleAudio,
        RecordingAction::ToggleCursor,
        RecordingAction::ToggleSettings,
    ] {
        push_action(&mut slots, &mut cursor, action, 30.0 * scale);
    }
    push_separator(&mut slots, &mut cursor);
    for tool in [
        Tool::Rect,
        Tool::Pen,
        Tool::Badge,
        Tool::Text,
        Tool::Line,
        Tool::Arrow,
        Tool::Spotlight,
        Tool::Mosaic,
    ] {
        push_tool(&mut slots, &mut cursor, tool);
    }
    push_special(
        &mut slots,
        &mut cursor,
        RecordingSlotKind::Chevron,
        14.0 * scale,
    );
    push_separator(&mut slots, &mut cursor);
    push_action(&mut slots, &mut cursor, RecordingAction::Undo, 30.0 * scale);
    push_separator(&mut slots, &mut cursor);
    for action in [
        RecordingAction::Move,
        RecordingAction::Close,
        RecordingAction::Restart,
    ] {
        push_action(&mut slots, &mut cursor, action, 30.0 * scale);
    }
    push_special(
        &mut slots,
        &mut cursor,
        RecordingSlotKind::DragHandle,
        24.0 * scale,
    );
    if let Some(slot) = slots.last_mut() {
        slot.item = Some(RecordingItem::Action(RecordingAction::DragHandle));
    }
    let width = cursor.x + 10.0 * scale;
    let offset = session.toolbar_offset();
    let x = placement_axis(
        selection.x + offset.0,
        selection.width,
        PlacementBounds {
            size: width,
            screen: screen_width,
            margin: 8.0 * scale,
        },
    );
    let y = placement_vertical(
        selection.y + offset.1,
        selection.height,
        PlacementBounds {
            size: bar_height,
            screen: screen_height,
            margin: 8.0 * scale,
        },
    );
    let bar_rect = Rect {
        x,
        y,
        width,
        height: bar_height,
    };
    for slot in &mut slots {
        slot.rect.x += x;
        slot.rect.y += y;
    }

    let mut menu_rect = None;
    let mut menu_rows = Vec::new();
    if session.settings_open() {
        let menu_width = 126.0 * scale;
        let row_height = 30.0 * scale;
        let padding = 6.0 * scale;
        let menu_height = padding * 2.0 + row_height * 3.0;
        let settings_rect = slots
            .iter()
            .find(|slot| slot.item == Some(RecordingItem::Action(RecordingAction::ToggleSettings)))
            .map(|slot| slot.rect);
        let preferred_x = settings_rect.map_or(bar_rect.x, |rect| rect.x - menu_width + rect.width);
        let x = preferred_x.clamp(
            8.0 * scale,
            (screen_width - menu_width - 8.0 * scale).max(8.0 * scale),
        );
        let y = if bar_rect.y >= menu_height + 6.0 * scale {
            bar_rect.y - menu_height - 6.0 * scale
        } else {
            (bar_rect.y + bar_rect.height + 6.0 * scale)
                .min((screen_height - menu_height - 8.0 * scale).max(8.0 * scale))
        };
        let rect = Rect {
            x,
            y,
            width: menu_width,
            height: menu_height,
        };
        menu_rect = Some(rect);
        for (index, format) in [
            RecordingFormat::Gif,
            RecordingFormat::WebP,
            RecordingFormat::Mp4,
        ]
        .into_iter()
        .enumerate()
        {
            menu_rows.push((
                format,
                Rect {
                    x: x + padding,
                    y: y + padding + row_height * index as f32,
                    width: menu_width - padding * 2.0,
                    height: row_height,
                },
            ));
        }
    }

    RecordingLayout {
        scale,
        bar_rect,
        slots,
        menu_rect,
        menu_rows,
    }
}

#[derive(Debug, Clone, Copy)]
struct PlacementBounds {
    size: f32,
    screen: f32,
    margin: f32,
}

fn placement_axis(start: f32, extent: f32, bounds: PlacementBounds) -> f32 {
    let preferred = start;
    let max = (bounds.screen - bounds.size - bounds.margin).max(bounds.margin);
    if extent >= bounds.size {
        preferred.min(max).max(bounds.margin)
    } else {
        (start + (extent - bounds.size) / 2.0).clamp(bounds.margin, max)
    }
}

fn placement_vertical(start: f32, extent: f32, bounds: PlacementBounds) -> f32 {
    let below = start + extent + bounds.margin;
    if below + bounds.size <= bounds.screen - bounds.margin {
        return below;
    }
    let above = start - bounds.size - bounds.margin;
    if above >= bounds.margin {
        return above;
    }
    (bounds.screen - bounds.size - bounds.margin).max(bounds.margin)
}

struct LayoutCursor {
    x: f32,
    scale: f32,
    bar_height: f32,
}

fn push_special(
    slots: &mut Vec<RecordingSlot>,
    cursor: &mut LayoutCursor,
    kind: RecordingSlotKind,
    width: f32,
) {
    slots.push(RecordingSlot {
        kind,
        item: None,
        rect: Rect {
            x: cursor.x,
            y: 0.0,
            width,
            height: cursor.bar_height,
        },
    });
    cursor.x += width;
}

fn push_action(
    slots: &mut Vec<RecordingSlot>,
    cursor: &mut LayoutCursor,
    action: RecordingAction,
    width: f32,
) {
    push_slot(slots, cursor, Some(RecordingItem::Action(action)), width);
}

fn push_tool(slots: &mut Vec<RecordingSlot>, cursor: &mut LayoutCursor, tool: Tool) {
    push_slot(
        slots,
        cursor,
        Some(RecordingItem::Tool(tool)),
        28.0 * cursor.scale,
    );
}

fn push_slot(
    slots: &mut Vec<RecordingSlot>,
    cursor: &mut LayoutCursor,
    item: Option<RecordingItem>,
    width: f32,
) {
    slots.push(RecordingSlot {
        kind: RecordingSlotKind::Item,
        item,
        rect: Rect {
            x: cursor.x,
            y: 0.0,
            width,
            height: cursor.bar_height,
        },
    });
    cursor.x += width;
}

fn push_separator(slots: &mut Vec<RecordingSlot>, cursor: &mut LayoutCursor) {
    slots.push(RecordingSlot {
        kind: RecordingSlotKind::Separator,
        item: None,
        rect: Rect {
            x: cursor.x,
            y: 0.0,
            width: 11.0 * cursor.scale,
            height: cursor.bar_height,
        },
    });
    cursor.x += 11.0 * cursor.scale;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::NoopScreenRecorder;
    use std::sync::Arc;

    fn test_session() -> RecordingSession {
        RecordingSession::new(Arc::new(NoopScreenRecorder), Arc::new(|_| {}))
    }

    #[test]
    fn test_layout_stays_inside_screen_and_hits_controls() {
        let session = test_session();
        let selection = Rect::new(10.0, 10.0, 300.0, 200.0);
        let layout = calculate_recording_layout(RecordingLayoutRequest {
            session: &session,
            selection: &selection,
            screen_width: 640.0,
            screen_height: 480.0,
            scale: 1.0,
        });
        assert!(layout.bar_rect.x >= 8.0 * layout.scale);
        assert!(layout.bar_rect.y + layout.bar_rect.height <= 480.0 - 8.0 * layout.scale);
        assert_eq!(
            layout.hit(
                layout
                    .slots
                    .iter()
                    .find(|slot| {
                        slot.item == Some(RecordingItem::Action(RecordingAction::ToggleCapture))
                    })
                    .map(|slot| slot.rect.x + slot.rect.width / 2.0)
                    .expect("录制按钮必须存在"),
                layout.bar_rect.y + layout.bar_rect.height / 2.0,
            ),
            Some(RecordingItem::Action(RecordingAction::ToggleCapture))
        );
        let drag_handle = layout
            .slots
            .iter()
            .find(|slot| slot.item == Some(RecordingItem::Action(RecordingAction::DragHandle)))
            .expect("最右侧拖动手柄必须存在");
        assert!(
            (drag_handle.rect.x + drag_handle.rect.width
                - layout.bar_rect.x
                - layout.bar_rect.width
                + 10.0 * layout.scale)
                .abs()
                < 0.5,
            "拖动手柄必须位于工具栏最右端"
        );
    }

    #[test]
    fn test_layout_shrinks_to_fit_high_dpi_small_screen() {
        let session = test_session();
        let selection = Rect::new(40.0, 40.0, 900.0, 500.0);
        let layout = calculate_recording_layout(RecordingLayoutRequest {
            session: &session,
            selection: &selection,
            screen_width: 1280.0,
            screen_height: 720.0,
            scale: 2.0,
        });
        assert!(layout.scale < 2.0);
        assert!(layout.bar_rect.x + layout.bar_rect.width <= 1280.0 - 8.0 * layout.scale);
    }

    #[test]
    fn test_settings_menu_exposes_all_formats() {
        let mut session = test_session();
        session.toggle_settings();
        let selection = Rect::new(20.0, 20.0, 400.0, 200.0);
        let layout = calculate_recording_layout(RecordingLayoutRequest {
            session: &session,
            selection: &selection,
            screen_width: 900.0,
            screen_height: 700.0,
            scale: 1.0,
        });
        assert_eq!(layout.menu_rows.len(), 3);
        for (format, rect) in &layout.menu_rows {
            assert_eq!(
                layout.hit_menu(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0),
                Some(*format)
            );
        }
    }
}
