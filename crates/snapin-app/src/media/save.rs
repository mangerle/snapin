//! 截图与贴图的保存流程：对话框、编码落盘、降级目录、通知与打开目录。
//!
//! 保存属于应用层用例：UI 只提交图像与保存意图，不关心默认目录、文件命名模板、
//! 系统对话框或保存后通知。编码落盘统一放入阻塞线程池，对话框与通知在异步
//! 上下文等待，避免阻塞后台运行时。

use snapin_config::config::{Config, OutputConfig};
use snapin_foundation::AppPaths;
use snapin_platform::{
    open_path_in_file_manager, pick_save_path, send_desktop_notification, write_image_to_clipboard,
};
use snapin_ports::MediaJobOutcome;
use snapin_runtime::TaskRuntime;
use snapin_storage::{
    ensure_output_directory, save_image_to_path, save_image_with_config, suggest_save_path,
};
use std::path::PathBuf;
use std::sync::Arc;

/// 贴图保存请求。
pub(super) struct SavePasterRequest {
    /// 要保存的最终图像
    pub(super) image: Arc<image::RgbaImage>,
    /// 是否弹出系统保存对话框
    pub(super) as_dialog: bool,
    /// 保存成功后是否打开所在目录
    pub(super) open_folder_after: bool,
}

/// 执行截图保存：按配置直接落盘或先弹出保存对话框，成功后同步复制到剪贴板。
pub(super) async fn execute_save_screenshot(
    image: Arc<image::RgbaImage>,
    config: &Config,
    runtime: &TaskRuntime,
) -> MediaJobOutcome {
    let chosen_path = if config.output.prompt_save_path {
        match pick_save_target(config, runtime).await {
            Ok(Some(path)) => Some(path),
            Ok(None) => return MediaJobOutcome::SaveCancelled,
            Err(message) => return MediaJobOutcome::Failed { message },
        }
    } else {
        None
    };

    let path = match save_image(image.clone(), chosen_path, &config.output, runtime).await {
        Ok(path) => path,
        Err(message) => return MediaJobOutcome::Failed { message },
    };

    let clipboard_image = image;
    let clipboard_result = runtime
        .run_blocking_detached(move || write_image_to_clipboard(&clipboard_image))
        .await;
    match clipboard_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("保存后同步复制到系统剪贴板失败: {error}"),
        Err(error) => tracing::warn!("保存后剪贴板任务异常: {error}"),
    }
    MediaJobOutcome::Saved { path }
}

/// 执行贴图保存：支持快捷保存与系统对话框两条路径。
pub(super) async fn execute_save_paster(
    request: SavePasterRequest,
    config: &Config,
    runtime: &TaskRuntime,
) -> MediaJobOutcome {
    let SavePasterRequest {
        image,
        as_dialog,
        open_folder_after,
    } = request;

    let chosen_path = if as_dialog {
        match pick_save_target(config, runtime).await {
            Ok(Some(path)) => Some(path),
            Ok(None) => return MediaJobOutcome::SaveCancelled,
            Err(message) => return MediaJobOutcome::Failed { message },
        }
    } else {
        None
    };

    let path = match save_image(image, chosen_path, &config.output, runtime).await {
        Ok(path) => path,
        Err(message) => return MediaJobOutcome::Failed { message },
    };

    if config.output.notify_on_save {
        let body = snapin_foundation::trf!(
            "贴图图像已保存至: {}",
            "Pin image saved to: {}",
            path.display()
        );
        if let Err(error) =
            send_desktop_notification(snapin_foundation::i18n::tr("Snapin 贴图已保存"), &body).await
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

/// 在阻塞线程池执行编码落盘。
async fn save_image(
    image: Arc<image::RgbaImage>,
    chosen_path: Option<PathBuf>,
    output: &OutputConfig,
    runtime: &TaskRuntime,
) -> Result<PathBuf, String> {
    let output = output.clone();
    runtime
        .run_blocking_detached(move || match chosen_path {
            Some(path) => save_image_to_path(&image, &path, &output)
                .map_err(|error| format!("保存到所选路径失败: {error}")),
            None => save_quick_with_fallback(&image, &output),
        })
        .await
        .map_err(|error| format!("保存任务异常: {error}"))?
}

/// 弹出系统保存对话框，建议文件名与初始目录沿用快捷保存配置。
async fn pick_save_target(
    config: &Config,
    runtime: &TaskRuntime,
) -> Result<Option<PathBuf>, String> {
    let output = config.output.clone();
    let suggestion = runtime
        .run_blocking_detached(move || {
            if let Err(error) = ensure_output_directory(&output) {
                tracing::warn!("准备快捷保存目录失败，保存对话框将使用系统默认位置: {error}");
            }
            suggest_save_path(&output)
        })
        .await
        .map_err(|error| format!("准备保存建议失败: {error}"))?;

    pick_save_path(&suggestion.file_name, Some(suggestion.directory.as_path()))
        .await
        .map_err(|error| format!("打开保存对话框失败: {error}"))
}

/// 按用户配置保存；配置路径失败时降级保存到 Pictures 目录。
fn save_quick_with_fallback(
    image: &image::RgbaImage,
    output: &OutputConfig,
) -> Result<PathBuf, String> {
    let primary_error = match save_image_with_config(image, output) {
        Ok(path) => return Ok(path),
        Err(error) => error,
    };
    tracing::warn!("快捷保存失败，降级到 Pictures 目录: {primary_error}");

    let pictures_dir = AppPaths::pictures_dir();
    std::fs::create_dir_all(&pictures_dir)
        .map_err(|error| format!("创建 Pictures 目录失败: {error}"))?;
    let fallback_path = pictures_dir.join(format!("snapin_{}.png", AppPaths::generate_unique_id()));
    save_image_to_path(image, &fallback_path, output)
        .map_err(|error| format!("降级保存图像失败: {error}"))
}
