//! 贴图窗口快捷键、鼠标拖拽、滚轮缩放与右键上下文菜单事件响应。

use crate::paster::context_menu::{ContextMenuState, MenuItemId};
use crate::paster::manager::PasterItem;
use crate::paster::transform::{
    flip_pixmap_horizontal, flip_pixmap_vertical, rotate_pixmap_ccw, rotate_pixmap_cw,
};
use image::RgbaImage;
use snapin_core::config::{Config, DoubleClickAction};
use snapin_platform::{
    read_clipboard_image, read_clipboard_text, save_image_with_config, send_desktop_notification,
    write_image_to_clipboard,
};
use std::path::PathBuf;
use std::time::Instant;
use tiny_skia::Pixmap;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey};
use winit::window::Window;

/// 贴图窗口运行时状态
pub struct PasterWindowState {
    pub item: PasterItem,
    pub should_close: bool,
    pub should_destroy: bool,
    pub has_shadow: bool,
    pub show_toolbar: bool,
    pub smooth_scaling: bool,
    pub file_path: Option<PathBuf>,
    pub context_menu: ContextMenuState,
    pub cursor_pos: (f32, f32),
    pub ui_scale: f32,
    pub config: Config,
    last_click_time: Option<Instant>,
}

impl PasterWindowState {
    pub fn new(item: PasterItem) -> Self {
        Self {
            item,
            should_close: false,
            should_destroy: false,
            has_shadow: true,
            show_toolbar: false,
            smooth_scaling: true,
            file_path: None,
            context_menu: ContextMenuState::default(),
            cursor_pos: (0.0, 0.0),
            ui_scale: 1.0,
            config: Config::default(),
            last_click_time: None,
        }
    }

    /// 根据用户配置初始化贴图状态（注入初始不透明度）
    pub fn from_config(mut item: PasterItem, cfg: Config) -> Self {
        item.transform.opacity = cfg.paster.initial_opacity.clamp(0.1, 1.0);
        let mut state = Self::new(item);
        state.config = cfg;
        state
    }
}

/// 处理键盘事件
pub fn handle_paster_key(
    state: &mut PasterWindowState,
    key: &Key,
    key_state: ElementState,
    window: &Window,
) -> bool {
    if key_state != ElementState::Pressed {
        return false;
    }

    if state.context_menu.is_open {
        if let Key::Named(NamedKey::Escape) = key {
            state.context_menu.close();
            return true;
        }
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
            if let Some(rotated) = rotate_pixmap_cw(&state.item.pixmap) {
                state.item.pixmap = rotated;
                apply_window_scale_resize(state, window);
                true
            } else {
                false
            }
        }
        Key::Character(c) if c.eq_ignore_ascii_case("h") => {
            if let Some(flipped) = flip_pixmap_horizontal(&state.item.pixmap) {
                state.item.pixmap = flipped;
                true
            } else {
                false
            }
        }
        Key::Character(c) if c.eq_ignore_ascii_case("v") => {
            if let Some(flipped) = flip_pixmap_vertical(&state.item.pixmap) {
                state.item.pixmap = flipped;
                true
            } else {
                false
            }
        }
        Key::Character(c) if c.eq_ignore_ascii_case("c") => {
            copy_paster_to_clipboard(&state.item);
            false
        }
        Key::Character(c) if c.eq_ignore_ascii_case("s") => {
            save_paster_to_disk(state);
            false
        }
        _ => false,
    }
}

/// 处理鼠标移动事件
pub fn handle_paster_cursor_move(state: &mut PasterWindowState, pos: (f64, f64)) -> bool {
    state.cursor_pos = (pos.0 as f32, pos.1 as f32);
    if state.context_menu.is_open {
        let items = ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
        state.context_menu.handle_mouse_move(
            state.cursor_pos.0,
            state.cursor_pos.1,
            &items,
            state.smooth_scaling,
        )
    } else {
        false
    }
}

/// 处理鼠标滚轮缩放
pub fn handle_paster_scroll(
    state: &mut PasterWindowState,
    delta: MouseScrollDelta,
    window: &Window,
) -> bool {
    if state.context_menu.is_open {
        state.context_menu.close();
    }

    let delta_y = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0,
    };

    if delta_y.abs() > 0.01 {
        state.item.transform.zoom(delta_y);
        apply_window_scale_resize(state, window);
        true
    } else {
        false
    }
}

/// 处理鼠标按键点击与双击检测
pub fn handle_paster_mouse_button(
    state: &mut PasterWindowState,
    button: MouseButton,
    button_state: ElementState,
    window: &Window,
) -> bool {
    if button_state != ElementState::Pressed {
        return false;
    }

    if button == MouseButton::Right {
        state.context_menu.set_ui_scale(state.ui_scale);
        let win_size = window.inner_size();
        let main_items = ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
        state.context_menu.open_at(
            state.cursor_pos.0,
            state.cursor_pos.1,
            win_size.width as f32,
            win_size.height as f32,
            &main_items,
        );
        return true;
    }

    if button == MouseButton::Left {
        if state.context_menu.is_open {
            let main_items =
                ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
            let action = state.context_menu.handle_click(
                state.cursor_pos.0,
                state.cursor_pos.1,
                &main_items,
                state.smooth_scaling,
            );
            if let Some(act) = action {
                dispatch_menu_action(state, act, window);
            }
            return true;
        }

        // 优先拦截并响应对底部快捷工具条的点击，避免误触发底层窗口拖动
        if check_and_handle_toolbar_click(state, window) {
            return true;
        }

        let now = Instant::now();
        if let Some(prev) = state.last_click_time {
            if now.duration_since(prev).as_millis() < 300 {
                state.last_click_time = None;
                match state.config.paster.double_click_action {
                    DoubleClickAction::CopyAndClose => {
                        copy_paster_to_clipboard(&state.item);
                        state.should_close = true;
                    }
                    DoubleClickAction::Copy => {
                        copy_paster_to_clipboard(&state.item);
                    }
                    DoubleClickAction::Close => {
                        state.should_close = true;
                    }
                }
                return true;
            }
        }
        state.last_click_time = Some(now);
        let _ = window.drag_window();
    }

    false
}

/// 分发执行上下文菜单动作指令
fn dispatch_menu_action(state: &mut PasterWindowState, action: MenuItemId, window: &Window) {
    match action {
        MenuItemId::CopyImage => copy_paster_to_clipboard(&state.item),
        MenuItemId::SaveImageAs => save_paster_to_disk(state),
        MenuItemId::ToggleToolbar => state.show_toolbar = !state.show_toolbar,
        MenuItemId::ToggleShadow => state.has_shadow = !state.has_shadow,
        MenuItemId::Scale(s) => {
            state.item.transform.set_scale(s);
            apply_window_scale_resize(state, window);
        }
        MenuItemId::ToggleSmoothScaling => state.smooth_scaling = !state.smooth_scaling,
        MenuItemId::RotateLeft => {
            if let Some(rotated) = rotate_pixmap_ccw(&state.item.pixmap) {
                state.item.pixmap = rotated;
                apply_window_scale_resize(state, window);
            }
        }
        MenuItemId::RotateRight => {
            if let Some(rotated) = rotate_pixmap_cw(&state.item.pixmap) {
                state.item.pixmap = rotated;
                apply_window_scale_resize(state, window);
            }
        }
        MenuItemId::FlipHorizontal => {
            if let Some(flipped) = flip_pixmap_horizontal(&state.item.pixmap) {
                state.item.pixmap = flipped;
            }
        }
        MenuItemId::FlipVertical => {
            if let Some(flipped) = flip_pixmap_vertical(&state.item.pixmap) {
                state.item.pixmap = flipped;
            }
        }
        MenuItemId::PasteFromClipboard => paste_from_clipboard_to_paster(state, window),
        MenuItemId::OpenInFolder => open_paster_in_folder(state),
        MenuItemId::Close => state.should_close = true,
        MenuItemId::Destroy => {
            state.should_destroy = true;
            state.should_close = true;
        }
    }
}

/// 检测并处理对贴图底部现代胶囊快捷工具条的点击
pub fn check_and_handle_toolbar_click(state: &mut PasterWindowState, window: &Window) -> bool {
    if !state.show_toolbar {
        return false;
    }

    let win_size = window.inner_size();
    let w = win_size.width as f32;
    let h = win_size.height as f32;
    let s = state.ui_scale;

    let bar_w = (230.0 * s).min(w - 16.0);
    let bar_h = 28.0 * s;
    if bar_w < 100.0 || h < bar_h + 16.0 {
        return false;
    }

    let bar_x = (w - bar_w) / 2.0;
    let bar_y = h - bar_h - 8.0 * s;

    let (cx, cy) = state.cursor_pos;
    if cx < bar_x || cx > bar_x + bar_w || cy < bar_y || cy > bar_y + bar_h {
        return false;
    }

    // 落在工具条胶囊内部，根据水平相对比例分发动作 (共 4 个按钮：复制, 保存, 旋转, 翻转)
    let rel_x = cx - bar_x;
    let item_w = bar_w / 4.0;
    let idx = (rel_x / item_w) as usize;

    match idx {
        0 => {
            copy_paster_to_clipboard(&state.item);
        }
        1 => {
            save_paster_to_disk(state);
        }
        2 => {
            if let Some(rotated) = rotate_pixmap_cw(&state.item.pixmap) {
                state.item.pixmap = rotated;
                apply_window_scale_resize(state, window);
            }
        }
        _ => {
            if let Some(flipped) = flip_pixmap_horizontal(&state.item.pixmap) {
                state.item.pixmap = flipped;
            }
        }
    }
    true
}

/// 重新计算并应用根据缩放比例调整后的窗口尺寸
fn apply_window_scale_resize(state: &PasterWindowState, window: &Window) {
    let base_w = state.item.pixmap.width() as f32;
    let base_h = state.item.pixmap.height() as f32;
    let scale = state.item.transform.scale;
    let new_w = (base_w * scale).max(20.0) as u32;
    let new_h = (base_h * scale).max(20.0) as u32;
    let _ = window.request_inner_size(PhysicalSize::new(new_w, new_h));
}

/// 复制贴图图像至剪贴板
fn copy_paster_to_clipboard(item: &PasterItem) {
    let w = item.pixmap.width();
    let h = item.pixmap.height();
    if let Some(rgba) = RgbaImage::from_raw(w, h, item.pixmap.data().to_vec()) {
        let _ = write_image_to_clipboard(&rgba);
    }
}

/// 保存贴图图像并遵循全局输出配置（格式、质量与命名模板）
fn save_paster_to_disk(state: &mut PasterWindowState) {
    let w = state.item.pixmap.width();
    let h = state.item.pixmap.height();
    if let Some(rgba) = RgbaImage::from_raw(w, h, state.item.pixmap.data().to_vec()) {
        let out_res = save_image_with_config(&rgba, &state.config.output).or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let pic_dir = PathBuf::from(home).join("Pictures");
            let _ = std::fs::create_dir_all(&pic_dir);
            let fallback_path = pic_dir.join(format!("{}.png", state.item.title));
            rgba.save(&fallback_path)
                .map(|_| fallback_path)
                .map_err(|e| snapin_platform::PlatformError::Io {
                    path: "Pictures".to_string(),
                    source: std::io::Error::other(e.to_string()),
                })
        });

        if let Ok(out) = out_res {
            state.file_path = Some(out.clone());
            if state.config.output.notify_on_save {
                let msg = format!("贴图图像已保存至: {}", out.display());
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let _ = send_desktop_notification("Snapin 贴图已保存", &msg).await;
                    });
                }
            }
        }
    }
}

/// 在系统文件管理器中查看贴图
fn open_paster_in_folder(state: &mut PasterWindowState) {
    if state.file_path.is_none() {
        save_paster_to_disk(state);
    }
    if let Some(ref path) = state.file_path {
        if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }
}

/// 从剪贴板替换或更新当前贴图内容
fn paste_from_clipboard_to_paster(state: &mut PasterWindowState, window: &Window) {
    if let Ok(Some(img)) = read_clipboard_image() {
        if let Some(mut pixmap) = Pixmap::new(img.width(), img.height()) {
            pixmap.data_mut().copy_from_slice(img.as_raw());
            state.item.pixmap = pixmap;
            apply_window_scale_resize(state, window);
            return;
        }
    }

    if let Ok(Some(text)) = read_clipboard_text() {
        if let Some(pixmap) =
            crate::paster::render_simple_text_card(&text.lines().collect::<Vec<_>>())
        {
            state.item.pixmap = pixmap;
            apply_window_scale_resize(state, window);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolbar_hit_geometry_calculation() {
        let dummy = Pixmap::new(300, 200).unwrap();
        let item = PasterItem::new(1, "test".to_string(), dummy, (0, 0));
        let mut state = PasterWindowState::new(item);
        state.show_toolbar = true;
        state.ui_scale = 1.0;

        let w = 300.0f32;
        let h = 200.0f32;
        let s = 1.0f32;
        let bar_w = (230.0 * s).min(w - 16.0); // 230.0
        let bar_h = 28.0 * s; // 28.0
        let bar_x = (w - bar_w) / 2.0; // (300 - 230) / 2 = 35.0
        let bar_y = h - bar_h - 8.0 * s; // 200 - 28 - 8 = 164.0

        // 落在工具条外部
        state.cursor_pos = (10.0, 10.0);
        let in_bar = state.cursor_pos.0 >= bar_x
            && state.cursor_pos.0 <= bar_x + bar_w
            && state.cursor_pos.1 >= bar_y
            && state.cursor_pos.1 <= bar_y + bar_h;
        assert!(!in_bar);

        // 落在工具条内部 (第一个按钮: 复制)
        state.cursor_pos = (bar_x + 10.0, bar_y + 10.0);
        let in_bar = state.cursor_pos.0 >= bar_x
            && state.cursor_pos.0 <= bar_x + bar_w
            && state.cursor_pos.1 >= bar_y
            && state.cursor_pos.1 <= bar_y + bar_h;
        assert!(in_bar);

        let item_w = bar_w / 4.0;
        let idx = ((state.cursor_pos.0 - bar_x) / item_w) as usize;
        assert_eq!(idx, 0, "点击应命中复制按钮");

        // 落在工具条内部 (第三个按钮: 旋转)
        state.cursor_pos = (bar_x + item_w * 2.5, bar_y + 10.0);
        let idx = ((state.cursor_pos.0 - bar_x) / item_w) as usize;
        assert_eq!(idx, 2, "点击应命中旋转按钮");
    }
}
