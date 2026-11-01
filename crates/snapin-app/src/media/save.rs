//! 截图与贴图的保存流程：对话框、编码落盘、降级目录、通知与打开目录。
//!
//! 保存属于应用层用例：UI 只提交图像与保存意图，不关心默认目录、文件命名模板、
//! 系统对话框或保存后通知。这样截图与贴图共享同一套错误处理与降级策略。

use snapin_core::config::Config;
use snapin_core::AppPaths;
use snapin_platform::{
    open_path_in_file_manager, pick_save_path, resolve_save_path, save_image_to_path,
    save_image_with_config, send_desktop_notification, write_image_to_clipboard,
};
use snapin_ui::MediaJobOutcome;
use std::path::{Path, PathBuf};

/// 贴图保存请求。
pub(super) struct SavePasterRequest {
    /// 要保存的最终图像
    pub(super) image: image::RgbaImage,
    /// 是否弹出系统保存对话框
    pub(super) as_dialog: bool,
    /// 保存成功后是否打开所在目录
    pub(super) open_folder_after: bool,
}

/// 执行截图保存：按配置直接落盘或先弹出保存对话框，成功后同步复制到剪贴板。
pub(super) fn execute_save_screenshot(
    image: &image::RgbaImage,
    config: &Config,
    runtime: &tokio::runtime::Runtime,
) -> MediaJobOutcome {
    let chosen_path = if config.output.prompt_save_path {
        match pick_save_target(config, runtime) {
            Ok(Some(path)) => Some(path),
            Ok(None) => return MediaJobOutcome::SaveCancelled,
            Err(message) => return MediaJobOutcome::Failed { message },
        }
    } else {
        None
    };

    let saved = match chosen_path {
        Some(path) => save_image_to_path(image, &path, &config.output)
            .map_err(|error| format!("保存到所选路径失败: {error}")),
        None => save_quick_with_fallback(image, config),
    };
    let path = match saved {
        Ok(path) => path,
        Err(message) => return MediaJobOutcome::Failed { message },
    };

    if let Err(error) = write_image_to_clipboard(image) {
        tracing::warn!("保存后同步复制到系统剪贴板失败: {error}");
    }
    MediaJobOutcome::Saved { path }
}

/// 执行贴图保存：支持快捷保存与系统对话框两条路径。
pub(super) fn execute_save_paster(
    request: SavePasterRequest,
    config: &Config,
    runtime: &tokio::runtime::Runtime,
) -> MediaJobOutcome {
    let SavePasterRequest {
        image,
        as_dialog,
        open_folder_after,
    } = request;

    let saved = if as_dialog {
        match pick_save_target(config, runtime) {
            Ok(Some(path)) => save_image_to_path(&image, &path, &config.output)
                .map_err(|error| format!("保存到所选路径失败: {error}")),
            Ok(None) => return MediaJobOutcome::SaveCancelled,
            Err(message) => return MediaJobOutcome::Failed { message },
        }
    } else {
        save_quick_with_fallback(&image, config)
    };
    let path = match saved {
        Ok(path) => path,
        Err(message) => return MediaJobOutcome::Failed { message },
    };

    if config.output.notify_on_save {
        let body = format!("贴图图像已保存至: {}", path.display());
        if let Err(error) = runtime.block_on(send_desktop_notification("Snapin 贴图已保存", &body))
        {
            tracing::debug!("发送贴图保存通知失败或系统通知不可用: {error}");
        }
    }
    if open_folder_after {
        if let Some(parent) = path.parent() {
            open_path_in_file_manager(parent);
        }
    }
    MediaJobOutcome::Saved { path }
}

/// 弹出系统保存对话框，建议文件名与初始目录沿用快捷保存配置。
fn pick_save_target(
    config: &Config,
    runtime: &tokio::runtime::Runtime,
) -> Result<Option<PathBuf>, String> {
    let suggested = resolve_save_path(&config.output);
    let suggested_name = suggested
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("snapin.png")
        .to_string();
    let current_folder = suggested
        .parent()
        .filter(|folder| folder.is_absolute())
        .map(Path::to_path_buf);

    runtime
        .block_on(pick_save_path(&suggested_name, current_folder.as_deref()))
        .map_err(|error| format!("打开保存对话框失败: {error}"))
}

/// 按用户配置保存；配置路径失败时降级保存到 Pictures 目录。
fn save_quick_with_fallback(image: &image::RgbaImage, config: &Config) -> Result<PathBuf, String> {
    if let Ok(path) = save_image_with_config(image, &config.output) {
        return Ok(path);
    }

    let pictures_dir = AppPaths::pictures_dir();
    std::fs::create_dir_all(&pictures_dir)
        .map_err(|error| format!("创建 Pictures 目录失败: {error}"))?;
    let fallback_path = pictures_dir.join(format!("snapin_{}.png", AppPaths::generate_unique_id()));
    image
        .save(&fallback_path)
        .map_err(|error| format!("降级保存图像失败: {error}"))?;
    Ok(fallback_path)
}
