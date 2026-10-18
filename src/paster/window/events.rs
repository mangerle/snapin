//! 贴图窗口快捷键、鼠标拖拽与滚轮缩放事件响应。

use crate::paster::manager::PasterItem;
use crate::platform::write_image_to_clipboard;
use image::RgbaImage;
use std::path::PathBuf;
use std::time::Instant;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey};
use winit::window::Window;

/// 贴图窗口运行时状态
pub struct PasterWindowState {
    pub item: PasterItem,
    pub should_close: bool,
    last_click_time: Option<Instant>,
}

impl PasterWindowState {
    pub fn new(item: PasterItem) -> Self {
        Self {
            item,
            should_close: false,
            last_click_time: None,
        }
    }
}

/// 处理键盘事件
pub fn handle_paster_key(
    state: &mut PasterWindowState,
    key: &Key,
    key_state: ElementState,
    _window: &Window,
) -> bool {
    if key_state != ElementState::Pressed {
        return false;
    }

    match key {
        Key::Named(NamedKey::Escape) => {
            state.should_close = true;
            true
        }
        Key::Character(c) if c == "1" => {
            state.item.transform.adjust_opacity(-0.1);
            true
        }
        Key::Character(c) if c == "2" => {
            state.item.transform.adjust_opacity(0.1);
            true
        }
        Key::Character(c) if c == "3" => {
            state.item.transform.rotate_clockwise();
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("h") => {
            state.item.transform.toggle_flip_horizontal();
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("v") => {
            state.item.transform.toggle_flip_vertical();
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("c") => {
            copy_paster_to_clipboard(&state.item);
            false
        }
        Key::Character(c) if c.eq_ignore_ascii_case("s") => {
            save_paster_to_disk(&state.item);
            false
        }
        _ => false,
    }
}

/// 处理鼠标滚轮缩放
pub fn handle_paster_scroll(
    state: &mut PasterWindowState,
    delta: MouseScrollDelta,
    window: &Window,
) -> bool {
    let delta_y = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0,
    };

    if delta_y.abs() > 0.01 {
        state.item.transform.zoom(delta_y);
        let base_w = state.item.pixmap.width() as f32;
        let base_h = state.item.pixmap.height() as f32;
        let scale = state.item.transform.scale;
        let new_w = (base_w * scale).max(20.0) as u32;
        let new_h = (base_h * scale).max(20.0) as u32;

        let _ = window.request_inner_size(PhysicalSize::new(new_w, new_h));
        true
    } else {
        false
    }
}

/// 处理鼠标点击与双击检测
pub fn handle_paster_mouse_button(
    state: &mut PasterWindowState,
    button: MouseButton,
    button_state: ElementState,
    window: &Window,
) -> bool {
    if button != MouseButton::Left {
        return false;
    }

    if button_state == ElementState::Pressed {
        let now = Instant::now();
        if let Some(prev) = state.last_click_time {
            if now.duration_since(prev).as_millis() < 300 {
                // 双击左键快速关闭贴图
                state.should_close = true;
                return true;
            }
        }
        state.last_click_time = Some(now);

        // 原生拖拽请求
        let _ = window.drag_window();
    }

    false
}

/// 复制贴图图像至剪贴板
fn copy_paster_to_clipboard(item: &PasterItem) {
    let w = item.pixmap.width();
    let h = item.pixmap.height();
    if let Some(rgba) = RgbaImage::from_raw(w, h, item.pixmap.data().to_vec()) {
        let _ = write_image_to_clipboard(&rgba);
    }
}

/// 保存贴图图像至 Pictures 目录
fn save_paster_to_disk(item: &PasterItem) {
    let w = item.pixmap.width();
    let h = item.pixmap.height();
    if let Some(rgba) = RgbaImage::from_raw(w, h, item.pixmap.data().to_vec()) {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let out = PathBuf::from(home)
            .join("Pictures")
            .join(format!("{}.png", item.title));
        let _ = rgba.save(&out);
    }
}
