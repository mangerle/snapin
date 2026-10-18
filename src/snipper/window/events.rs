//! 截图会话键盘与鼠标事件响应及状态机流转。

use super::types::SnipperOutcome;
use crate::annotation::shapes::{ArrowAction, MarkerAction, MosaicAction, PenAction, RectAction};
use crate::annotation::AnnotationHistory;
use crate::paster::PasterItem;
use crate::platform::{write_image_to_clipboard, write_text_to_clipboard};
use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{DragMode, Rect, SelectionHistory, SelectionState};
use crate::snipper::toolbar::{Toolbar, ToolbarItem};
use image::{GenericImageView, RgbaImage};
use std::path::PathBuf;
use winit::event::{ElementState, MouseButton};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::Window;

/// 截图会话可变运行状态封装
pub struct SnipperState {
    pub selection: SelectionState,
    pub history: SelectionHistory,
    pub magnifier: Magnifier,
    pub toolbar: Toolbar,
    pub annotations: AnnotationHistory,
    pub cursor_pos: (f64, f64),
    pub modifiers: ModifiersState,
    pub pending_stroke_points: Vec<(f32, f32)>,
    pub outcome: Option<SnipperOutcome>,
}

impl SnipperState {
    pub fn new() -> Self {
        Self {
            selection: SelectionState::Idle,
            history: SelectionHistory::default(),
            magnifier: Magnifier::default(),
            toolbar: Toolbar::default(),
            annotations: AnnotationHistory::default(),
            cursor_pos: (0.0, 0.0),
            modifiers: ModifiersState::default(),
            pending_stroke_points: Vec::with_capacity(64),
            outcome: None,
        }
    }
}

impl Default for SnipperState {
    fn default() -> Self {
        Self::new()
    }
}

/// 处理键盘按键交互
pub fn handle_key_input(
    state: &mut SnipperState,
    key: &Key,
    key_state: ElementState,
    original_rgba: &RgbaImage,
) -> bool {
    if key_state != ElementState::Pressed {
        return false;
    }

    match key {
        Key::Named(NamedKey::Escape) => handle_escape(state),
        Key::Named(NamedKey::Shift) => {
            state.magnifier.format = state.magnifier.format.next();
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("c") && !state.modifiers.control_key() => {
            handle_copy_hex_color(state, original_rgba);
            false
        }
        Key::Character(c) if c.eq_ignore_ascii_case("z") && state.modifiers.control_key() => {
            state.annotations.undo()
        }
        Key::Character(c) if c.eq_ignore_ascii_case("y") && state.modifiers.control_key() => {
            state.annotations.redo()
        }
        Key::Character(c) if c.eq_ignore_ascii_case("c") && state.modifiers.control_key() => {
            handle_action_copy(state, original_rgba);
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("s") && state.modifiers.control_key() => {
            handle_action_save(state, original_rgba);
            true
        }
        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::F3) => {
            handle_action_pin(state, original_rgba);
            true
        }
        Key::Character(c) if c == "," => handle_history_back(state, original_rgba),
        Key::Character(c) if c == "." => handle_history_forward(state, original_rgba),
        _ => false,
    }
}

/// 逐级退出处理 (先收起二级栏/工具 -> 再取消选区 -> 再退出窗口)
fn handle_escape(state: &mut SnipperState) -> bool {
    if state.toolbar.active_tool.is_some() {
        state.toolbar.active_tool = None;
        state.toolbar.sub_rect = None;
        true
    } else if matches!(state.selection, SelectionState::Locked { .. }) {
        state.selection = SelectionState::Idle;
        true
    } else {
        state.outcome = Some(SnipperOutcome::Cancelled);
        true
    }
}

/// 复制放大镜中心取色结果
fn handle_copy_hex_color(state: &SnipperState, img: &RgbaImage) {
    let px = (state.cursor_pos.0 as i32).clamp(0, img.width().saturating_sub(1) as i32) as u32;
    let py = (state.cursor_pos.1 as i32).clamp(0, img.height().saturating_sub(1) as i32) as u32;
    let pixel = img.get_pixel(px, py);
    let hex = format!("#{:02X}{:02X}{:02X}", pixel[0], pixel[1], pixel[2]);
    let _ = write_text_to_clipboard(&hex);
}

/// 历史选区回溯向后
fn handle_history_back(state: &mut SnipperState, img: &RgbaImage) -> bool {
    if let Some(r) = state.history.previous_history() {
        state.selection = SelectionState::Locked {
            rect: r,
            active_drag: None,
        };
        state
            .toolbar
            .update_layout(&r, img.width() as f32, img.height() as f32);
        true
    } else {
        false
    }
}

/// 历史选区回溯向前
fn handle_history_forward(state: &mut SnipperState, img: &RgbaImage) -> bool {
    if let Some(r) = state.history.next_history() {
        state.selection = SelectionState::Locked {
            rect: r,
            active_drag: None,
        };
        state
            .toolbar
            .update_layout(&r, img.width() as f32, img.height() as f32);
        true
    } else {
        false
    }
}

/// 鼠标移动事件统一处理
pub fn handle_cursor_move(
    state: &mut SnipperState,
    pos: (f64, f64),
    _window: &Window,
    screen_w: f32,
    screen_h: f32,
) -> bool {
    state.cursor_pos = pos;
    let (cx, cy) = (pos.0 as f32, pos.1 as f32);

    match &mut state.selection {
        SelectionState::Idle => {
            // 工具栏不在活跃状态，请求重绘放大镜
            true
        }
        SelectionState::Creating { current, .. } => {
            *current = (cx, cy);
            true
        }
        SelectionState::Locked { rect, active_drag } => {
            if let Some(drag) = active_drag {
                // 正在拖拽手柄平移或缩放
                let p = (cx, cy);
                let (r, d) = match drag {
                    DragMode::Move { offset_x, offset_y } => (
                        Rect {
                            x: p.0 - *offset_x as f32,
                            y: p.1 - *offset_y as f32,
                            ..*rect
                        },
                        DragMode::Move {
                            offset_x: *offset_x,
                            offset_y: *offset_y,
                        },
                    ),
                    DragMode::Resize(h) => {
                        (Rect::from_points((rect.x, rect.y), p), DragMode::Resize(*h))
                    }
                };
                *rect = r;
                *active_drag = Some(d);
                state.toolbar.update_layout(rect, screen_w, screen_h);
                true
            } else {
                state.toolbar.handle_mouse_move(cx, cy)
            }
        }
    }
}

/// 鼠标按键点击事件处理
pub fn handle_mouse_button(
    state: &mut SnipperState,
    button: MouseButton,
    button_state: ElementState,
    original_rgba: &RgbaImage,
) -> bool {
    let (cx, cy) = (state.cursor_pos.0 as f32, state.cursor_pos.1 as f32);

    if button == MouseButton::Right && button_state == ElementState::Pressed {
        return handle_escape(state);
    }

    if button != MouseButton::Left {
        return false;
    }

    match button_state {
        ElementState::Pressed => handle_left_press(state, cx, cy, original_rgba),
        ElementState::Released => handle_left_release(state, cx, cy, original_rgba),
    }
}

/// 鼠标左键按下交互
fn handle_left_press(
    state: &mut SnipperState,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) -> bool {
    if matches!(state.selection, SelectionState::Locked { .. }) {
        // 先检测工具栏点击命中
        if let Some(item) = state.toolbar.handle_click(cx, cy) {
            return dispatch_toolbar_action(state, item, original_rgba);
        }

        // 若处于标注模式，记录标注笔触起点
        if state.toolbar.active_tool.is_some() {
            state.pending_stroke_points.clear();
            state.pending_stroke_points.push((cx, cy));
            return true;
        }
    }

    match state.selection {
        SelectionState::Idle => {
            state.selection = SelectionState::Creating {
                start: (cx, cy),
                current: (cx, cy),
            };
            true
        }
        SelectionState::Locked {
            rect,
            ref mut active_drag,
        } if rect.contains(cx, cy) => {
            *active_drag = Some(DragMode::Move {
                offset_x: (cx - rect.x) as i32,
                offset_y: (cy - rect.y) as i32,
            });
            true
        }
        _ => false,
    }
}

/// 鼠标左键抬起交互
fn handle_left_release(
    state: &mut SnipperState,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) -> bool {
    let screen_w = original_rgba.width() as f32;
    let screen_h = original_rgba.height() as f32;

    match &mut state.selection {
        SelectionState::Creating { start, current } => {
            let rect = Rect::from_points(*start, *current);
            if rect.width > 4.0 && rect.height > 4.0 {
                state.history.push(rect);
                state.toolbar.update_layout(&rect, screen_w, screen_h);
                state.selection = SelectionState::Locked {
                    rect,
                    active_drag: None,
                };
            } else {
                state.selection = SelectionState::Idle;
            }
            true
        }
        SelectionState::Locked { rect, active_drag } => {
            if active_drag.is_some() {
                *active_drag = None;
                state.history.push(*rect);
                state.toolbar.update_layout(rect, screen_w, screen_h);
                true
            } else if state.toolbar.active_tool.is_some() && !state.pending_stroke_points.is_empty()
            {
                state.pending_stroke_points.push((cx, cy));
                commit_annotation_action(state);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// 将暂存的笔触图元提交至 Undo 栈
fn commit_annotation_action(state: &mut SnipperState) {
    if let Some(tool) = state.toolbar.active_tool {
        let pts = &state.pending_stroke_points;
        if pts.len() < 2 {
            state.pending_stroke_points.clear();
            return;
        }

        let stroke_w = state.toolbar.sub_options.stroke_width.to_pixel();
        let c = state.toolbar.sub_options.color;
        let rgba = (
            (c.red() * 255.0).round() as u8,
            (c.green() * 255.0).round() as u8,
            (c.blue() * 255.0).round() as u8,
            (c.alpha() * 255.0).round() as u8,
        );

        match tool {
            ToolbarItem::ToolRect => {
                let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
                state.annotations.push(Box::new(RectAction {
                    rect: (rect.x, rect.y, rect.width, rect.height),
                    color: rgba,
                    stroke_width: stroke_w,
                    is_filled: false,
                }));
            }
            ToolbarItem::ToolArrow => {
                state.annotations.push(Box::new(ArrowAction {
                    start: pts[0],
                    end: pts[pts.len() - 1],
                    color: rgba,
                    stroke_width: stroke_w,
                }));
            }
            ToolbarItem::ToolPen => {
                state.annotations.push(Box::new(PenAction {
                    points: pts.clone(),
                    color: rgba,
                    stroke_width: stroke_w,
                }));
            }
            ToolbarItem::ToolMarker => {
                state.annotations.push(Box::new(MarkerAction {
                    points: pts.clone(),
                    color: (rgba.0, rgba.1, rgba.2),
                    stroke_width: stroke_w * 2.0,
                }));
            }
            ToolbarItem::ToolMosaic => {
                let rect = Rect::from_points(pts[0], pts[pts.len() - 1]);
                state.annotations.push(Box::new(MosaicAction {
                    rect: (rect.x, rect.y, rect.width, rect.height),
                    cell_size: 10,
                }));
            }
            _ => {}
        }
    }
    state.pending_stroke_points.clear();
}

/// 执行工具栏动作派发
fn dispatch_toolbar_action(
    state: &mut SnipperState,
    item: ToolbarItem,
    original_rgba: &RgbaImage,
) -> bool {
    match item {
        ToolbarItem::ActionUndo => state.annotations.undo(),
        ToolbarItem::ActionRedo => state.annotations.redo(),
        ToolbarItem::ActionCopy => {
            handle_action_copy(state, original_rgba);
            true
        }
        ToolbarItem::ActionSave => {
            handle_action_save(state, original_rgba);
            true
        }
        ToolbarItem::ActionPin => {
            handle_action_pin(state, original_rgba);
            true
        }
        ToolbarItem::ActionClose => {
            state.outcome = Some(SnipperOutcome::Cancelled);
            true
        }
        _ => true,
    }
}

/// 裁剪当前选区图像
fn crop_selection_image(state: &SnipperState, original_rgba: &RgbaImage) -> Option<RgbaImage> {
    if let SelectionState::Locked { rect, .. } = &state.selection {
        let (px, py, pw, ph) = rect.to_pixel_bounds(original_rgba.width(), original_rgba.height());
        if pw > 0 && ph > 0 {
            let sub = original_rgba.view(px, py, pw, ph).to_image();
            return Some(sub);
        }
    }
    None
}

/// 复制选区图像到剪贴板并准备退出
fn handle_action_copy(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        let _ = write_image_to_clipboard(&cropped);
        state.outcome = Some(SnipperOutcome::Copied(cropped));
    }
}

/// 保存选区图像至文件
fn handle_action_save(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        let pictures_dir = std::env::var("HOME")
            .map(|h| PathBuf::from(h).join("Pictures"))
            .unwrap_or_else(|_| PathBuf::from("."));
        let filename = format!("snapin_{}.png", chrono_stamp());
        let out_path = pictures_dir.join(filename);
        let _ = cropped.save(&out_path);
        let _ = write_image_to_clipboard(&cropped);
        state.outcome = Some(SnipperOutcome::Saved(out_path));
    }
}

/// 将选区转为贴图固定在屏幕
fn handle_action_pin(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if let SelectionState::Locked { rect, .. } = &state.selection {
        if let Some(cropped) = crop_selection_image(state, original_rgba) {
            if let Some(mut pixmap) = tiny_skia::Pixmap::new(cropped.width(), cropped.height()) {
                pixmap.data_mut().copy_from_slice(cropped.as_raw());
                let id = chrono_stamp();
                let item = PasterItem::new(
                    id,
                    format!("pin_{id}"),
                    pixmap,
                    (rect.x as i32, rect.y as i32),
                );
                state.outcome = Some(SnipperOutcome::Pinned(item));
            }
        }
    }
}

/// 毫秒时间戳生成
fn chrono_stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
