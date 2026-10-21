//! 贴图窗口上下文菜单命令与磁盘/剪贴板动作分发模块。

use super::state::PasterWindowState;
use crate::paster::context_menu::MenuItemId;
use crate::paster::manager::PasterItem;
use image::RgbaImage;
use snapin_platform::{
    open_path_in_file_manager, read_clipboard_image, read_clipboard_text, save_image_with_config,
    send_desktop_notification, write_raw_image_to_clipboard,
};
use std::path::PathBuf;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 分发执行上下文菜单动作指令
pub fn dispatch_menu_action(state: &mut PasterWindowState, action: MenuItemId, window: &Window) {
    match action {
        MenuItemId::CopyImage => copy_paster_to_clipboard(&state.item),
        MenuItemId::SaveImageAs => save_paster_to_disk(state),
        MenuItemId::ToggleToolbar => state.set_toolbar_visible(!state.show_toolbar, window),
        MenuItemId::ToggleShadow => state.has_shadow = !state.has_shadow,
        MenuItemId::Scale(s) => state.set_scale(s, window),
        MenuItemId::ToggleSmoothScaling => state.smooth_scaling = !state.smooth_scaling,
        MenuItemId::RotateLeft => {
            state.rotate_ccw(window);
        }
        MenuItemId::RotateRight => {
            state.rotate_cw(window);
        }
        MenuItemId::FlipHorizontal => {
            state.flip_horizontal();
        }
        MenuItemId::FlipVertical => {
            state.flip_vertical();
        }
        MenuItemId::PasteFromClipboard => paste_from_clipboard_to_paster(state, window),
        MenuItemId::OpenInFolder => open_paster_in_folder(state),
        MenuItemId::Close => state.should_close = true,
    }
}

/// 复制贴图图像至剪贴板（基于零拷贝裸像素切片直接写入系统剪贴板）
pub fn copy_paster_to_clipboard(item: &PasterItem) {
    let w = item.pixmap.width();
    let h = item.pixmap.height();
    let _ = write_raw_image_to_clipboard(w, h, item.pixmap.data());
}

/// 保存贴图图像并遵循全局输出配置（格式、质量与命名模板）
pub fn save_paster_to_disk(state: &mut PasterWindowState) {
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
pub fn open_paster_in_folder(state: &mut PasterWindowState) {
    if state.file_path.is_none() {
        save_paster_to_disk(state);
    }
    if let Some(ref path) = state.file_path {
        if let Some(parent) = path.parent() {
            open_path_in_file_manager(parent);
        }
    }
}

/// 从剪贴板替换或更新当前贴图内容
pub fn paste_from_clipboard_to_paster(state: &mut PasterWindowState, window: &Window) {
    if let Ok(Some(img)) = read_clipboard_image() {
        if let Some(mut pixmap) = Pixmap::new(img.width(), img.height()) {
            pixmap.data_mut().copy_from_slice(img.as_raw());
            state.replace_pixmap(pixmap, window);
            return;
        }
    }

    if let Ok(Some(text)) = read_clipboard_text() {
        let pixmap = if let Some((r, g, b)) = crate::paster::try_parse_hex_color(&text) {
            crate::paster::render_color_swatch_card(r, g, b)
        } else {
            crate::paster::render_simple_text_card(&text.lines().collect::<Vec<_>>())
        };
        if let Some(pixmap) = pixmap {
            state.replace_pixmap(pixmap, window);
        }
    }
}
