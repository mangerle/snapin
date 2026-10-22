//! 贴图窗口上下文菜单命令与磁盘/剪贴板动作分发模块。

use super::state::PasterWindowState;
use crate::paster::context_menu::MenuItemId;
use crate::paster::workspace::proxy::PasterWorkspaceProxy;
use image::RgbaImage;
use snapin_core::config::OutputConfig;
use snapin_platform::{
    open_path_in_file_manager, pick_save_path, read_clipboard_image, read_clipboard_text,
    resolve_save_path, save_image_to_path, save_image_with_config, send_desktop_notification,
    write_raw_image_to_clipboard,
};
use std::path::{Path, PathBuf};
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

/// 复制贴图图像至剪贴板，并按配置发送成功通知。
///
/// 返回是否复制成功：双击“复制并关闭”等复合动作可据此决定是否继续关闭窗口，
/// 避免复制失败后仍关闭窗口导致用户丢失图像。
pub fn copy_paster_to_clipboard(state: &mut PasterWindowState) -> bool {
    let w = state.item.pixmap.width();
    let h = state.item.pixmap.height();
    match write_raw_image_to_clipboard(w, h, state.item.pixmap.data()) {
        Ok(()) => {
            state.set_status_message("已复制图像到剪贴板");
            if state.config.output.notify_on_copy {
                let message = "贴图图像已复制到剪贴板".to_string();
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let _ = send_desktop_notification("Snapin 贴图", &message).await;
                    });
                }
            }
            true
        }
        Err(error) => {
            tracing::warn!("复制贴图图像到系统剪贴板失败: {error}");
            state.set_status_message(format!("复制图像失败: {error}"));
            false
        }
    }
}

/// 保存贴图图像并遵循全局输出配置（格式、质量与命名模板）
pub fn save_paster_to_disk(state: &mut PasterWindowState) {
    let w = state.item.pixmap.width();
    let h = state.item.pixmap.height();
    let Some(rgba) = RgbaImage::from_raw(w, h, state.item.pixmap.data().to_vec()) else {
        state.set_status_message("读取贴图像素失败，无法保存");
        return;
    };

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

    match out_res {
        Ok(out) => {
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
        Err(error) => {
            tracing::warn!("贴图快捷保存失败: {error}");
            state.set_status_message(format!("保存图像失败: {error}"));
        }
    }
}

/// 弹出 XDG Portal 保存对话框，将当前贴图另存到用户选择的位置。
///
/// 对话框与文件编码都在后台执行，完成后通过工作区事件回填 `file_path`、状态提示
/// 与保存通知，避免阻塞 winit 事件循环。
pub fn save_paster_as(state: &mut PasterWindowState) {
    let Some(proxy) = state.workspace_proxy.clone() else {
        state.set_status_message("另存为不可用：工作区通道未就绪");
        return;
    };

    let (width, height) = (state.item.pixmap.width(), state.item.pixmap.height());
    let Some(image) = RgbaImage::from_raw(width, height, state.item.pixmap.data().to_vec()) else {
        state.set_status_message("读取贴图像素失败，无法另存为");
        return;
    };

    let suggested = resolve_save_path(&state.config.output);
    let suggested_name = suggested
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("snapin.png")
        .to_string();
    let current_folder = suggested
        .parent()
        .filter(|folder| folder.is_absolute())
        .map(Path::to_path_buf);
    let output = state.config.output.clone();
    let paster_id = state.item.id;

    state.set_status_message("正在选择保存位置...");
    spawn_save_as_task(
        proxy,
        paster_id,
        image,
        suggested_name,
        current_folder,
        output,
    );
}

/// 在后台运行 Portal 对话框与编码任务。
///
/// 工作区线程通常已进入守护进程的 Tokio 运行时，可直接 spawn；若调用方没有
/// Tokio 上下文（若干独立贴图场景），回退到自带 current-thread 运行时的短线程。
fn spawn_save_as_task(
    proxy: PasterWorkspaceProxy,
    paster_id: u64,
    image: RgbaImage,
    suggested_name: String,
    current_folder: Option<PathBuf>,
    output: OutputConfig,
) {
    let task = async move {
        let result = match pick_save_path(&suggested_name, current_folder.as_deref()).await {
            Ok(Some(path)) => {
                tokio::task::spawn_blocking(move || save_image_to_path(&image, &path, &output))
                    .await
                    .map_err(|error| format!("另存为任务调度失败: {error}"))
                    .and_then(|result| result.map_err(|error| error.to_string()))
                    .map(Some)
            }
            Ok(None) => Ok(None),
            Err(error) => Err(format!("打开保存对话框失败: {error}")),
        };

        if let Err(error) = proxy.save_as_finished(paster_id, result) {
            tracing::warn!("另存为结果回传工作区失败: {error}");
        }
    };

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(task);
        return;
    }

    let spawn_result = std::thread::Builder::new()
        .name("snapin-save-as".to_string())
        .spawn(move || {
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(task),
                Err(error) => tracing::warn!("创建另存为异步运行时失败: {error}"),
            }
        });
    if let Err(error) = spawn_result {
        tracing::warn!("派生另存为后台线程失败: {error}");
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
