//! 贴图窗口上下文菜单命令与磁盘/剪贴板动作分发模块。
//!
//! UI 只负责把用户动作转换为端口调用或媒体任务；保存对话框、图像编码、
//! 桌面通知与文件管理器打开统一由应用层完成。

use super::jobs::pixmap_to_rgba_image;
use super::state::PasterWindowState;
use crate::paster::context_menu::MenuItemId;
use image::RgbaImage;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 分发执行上下文菜单动作指令
pub fn dispatch_menu_action(state: &mut PasterWindowState, action: MenuItemId, window: &Window) {
    match action {
        MenuItemId::CopyImage => {
            copy_paster_to_clipboard(state);
        }
        MenuItemId::SaveImageAs => save_paster_as(state),
        MenuItemId::ToggleToolbar => {
            state.set_toolbar_visible(!state.show_toolbar, window);
            // 隐藏工具条会结束文字会话，同步关闭 IME，避免输入法状态残留
            if !state.show_toolbar {
                window.set_ime_allowed(false);
            }
        }
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

/// 将当前贴图转换为 RGBA 图像，失败时写入状态提示并返回 `None`。
fn paster_image(state: &mut PasterWindowState, operation: &str) -> Option<RgbaImage> {
    match pixmap_to_rgba_image(&state.item.pixmap) {
        Ok(image) => Some(image),
        Err(message) => {
            state.set_status_message(format!("读取贴图像素失败，无法{operation}: {message}"));
            None
        }
    }
}

/// 复制贴图图像至剪贴板。
///
/// 返回是否复制成功：双击“复制并关闭”等复合动作可据此决定是否继续关闭窗口，
/// 避免复制失败后仍关闭窗口导致用户丢失图像。
pub fn copy_paster_to_clipboard(state: &mut PasterWindowState) -> bool {
    let Some(image) = paster_image(state, "复制") else {
        return false;
    };
    match state.desktop.write_image_clipboard(&image) {
        Ok(()) => {
            state.set_status_message("已复制图像到剪贴板");
            state.desktop.notify_copy("贴图图像已复制到剪贴板");
            true
        }
        Err(error) => {
            tracing::warn!("复制贴图图像到系统剪贴板失败: {error}");
            state.set_status_message(format!("复制图像失败: {error}"));
            false
        }
    }
}

/// 保存贴图图像并遵循全局输出配置（格式、质量与命名模板）。
pub fn save_paster_to_disk(state: &mut PasterWindowState) {
    submit_paster_save(state, false, false);
}

/// 弹出系统保存对话框，将当前贴图另存到用户选择的位置。
pub fn save_paster_as(state: &mut PasterWindowState) {
    submit_paster_save(state, true, false);
}

/// 提交贴图保存任务。
///
/// 保存对话框、编码落盘、通知与可选的打开目录都由应用层媒体执行器完成；
/// 结果通过工作区用户事件按 `paster_id` 回填。
fn submit_paster_save(state: &mut PasterWindowState, as_dialog: bool, open_folder_after: bool) {
    if state.job_pending {
        state.set_status_message("请等待当前任务完成");
        return;
    }
    let Some(runtime) = state.job_runtime.clone() else {
        state.set_status_message("后台任务媒体执行器不可用");
        return;
    };
    let Some(image) = paster_image(state, "保存") else {
        return;
    };

    match runtime.submit_save(state.item.id, image, as_dialog, open_folder_after) {
        Ok(_) => {
            state.job_pending = true;
            state.set_status_message(if as_dialog {
                "正在选择保存位置..."
            } else {
                "正在保存图像..."
            });
        }
        Err(error) => {
            tracing::warn!("提交贴图保存任务失败: {error}");
            state.set_status_message(format!("提交保存任务失败: {error}"));
        }
    }
}

/// 在系统文件管理器中查看贴图。
///
/// 已有落盘路径时直接打开；否则先提交“保存并打开目录”任务，由应用层在保存成功后
/// 打开所在目录，避免 UI 同步编码与磁盘写入。
pub fn open_paster_in_folder(state: &mut PasterWindowState) {
    if let Some(ref path) = state.file_path {
        if let Some(parent) = path.parent() {
            state.desktop.open_in_file_manager(parent.to_path_buf());
        }
        return;
    }
    submit_paster_save(state, false, true);
}

/// 从剪贴板替换或更新当前贴图内容。
pub fn paste_from_clipboard_to_paster(state: &mut PasterWindowState, window: &Window) {
    if let Some(img) = state.desktop.read_clipboard_image() {
        if let Some(mut pixmap) = Pixmap::new(img.width(), img.height()) {
            pixmap.data_mut().copy_from_slice(img.as_raw());
            state.replace_pixmap(pixmap, window);
            return;
        }
    }

    if let Some(text) = state.desktop.read_clipboard_text() {
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
