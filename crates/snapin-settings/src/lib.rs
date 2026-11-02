//! Snapin 偏好设置中心图形界面模块（基于 Slint 声明式 GUI）。
//!
//! 采用独立轻量进程架构，提供现代化的卡片式设置面板，
//! 负责配置文件双向响应式同步、快捷键同步与系统集成。

slint::include_modules!();

mod callbacks;
mod clipboard_panel;
mod config_store;
pub mod config_sync;
pub mod mapping;
pub mod shortcut;
pub mod update_controller;

pub use clipboard_panel::run_clipboard_history_panel;

use config_store::ConfigStore;
use config_sync::{extract_config_from_window, sync_config_to_window};

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use snapin_config::config::{self, Config};
use snapin_daemon::notify_daemon_reload_config;
use snapin_foundation::error::CoreError;
use snapin_platform::{
    get_platform_capabilities, open_path_in_file_manager, pick_config_export_path,
    pick_config_import_path, sync_autostart, sync_hotkeys_to_gnome, write_image_to_clipboard,
    write_text_to_clipboard, PlatformError,
};
use snapin_runtime::{CancellationToken, TaskRuntime};
use snapin_storage::{
    clear_clipboard_history, load_clipboard_history, load_clipboard_image, ClipboardEntry,
    ClipboardEntryKind,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// 偏好设置模块领域错误
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SettingsError {
    /// 设置窗口初始化失败
    #[error("初始化设置中心图形窗口失败: {0}")]
    InitFailed(String),

    /// 设置中心事件循环运行异常
    #[error("设置中心事件循环运行异常: {0}")]
    RuntimeFailed(String),

    /// 核心基础库配置异常
    #[error(transparent)]
    Core(#[from] CoreError),

    /// 平台服务交互异常
    #[error(transparent)]
    Platform(#[from] PlatformError),

    /// 统一后台任务运行时初始化失败
    #[error("初始化设置中心后台任务运行时失败: {0}")]
    Runtime(String),
}

/// 运行 Slint 偏好设置中心图形界面
pub fn run_settings_gui() -> Result<(), SettingsError> {
    let window = create_settings_window()?;
    configure_settings_window(&window);
    let runtime = settings_runtime()?;
    let cancel = runtime.cancellation_token();
    initialize_settings_state(&window, &runtime, &cancel)?;
    let run_res = window
        .run()
        .map_err(|e| SettingsError::RuntimeFailed(e.to_string()));
    cancel.cancel();
    shutdown_settings_runtime(&runtime);
    restore_latest_hotkeys();
    run_res
}

/// 创建设置主窗口。
fn create_settings_window() -> Result<SettingsWindow, SettingsError> {
    SettingsWindow::new().map_err(|error| SettingsError::InitFailed(error.to_string()))
}

/// 配置窗口尺寸、应用标识、能力提示与快捷键格式化回调。
fn configure_settings_window(window: &SettingsWindow) {
    #[cfg(target_os = "linux")]
    let _ = slint::set_xdg_app_id("snapin");
    window
        .window()
        .set_size(slint::LogicalSize::new(900.0, 620.0));
    window.set_app_version(SharedString::from(env!("CARGO_PKG_VERSION")));
    window.set_extension_status_text(SharedString::from(extension_status_text()));
    register_shortcut_formatter(window);
}

/// 当前平台的扩展或原生能力提示。
fn extension_status_text() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        if get_platform_capabilities().gnome_extension_active {
            "GNOME Shell 窗口特权扩展已在线运行（贴图可强制置顶并精确落位）"
        } else {
            "GNOME Shell 窗口特权扩展未激活（贴图无法强制置顶与精确落位）"
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        "系统原生置顶与热键驱动已就绪"
    }
}

/// 注册 Slint 全局快捷键按键格式化回调。
fn register_shortcut_formatter(window: &SettingsWindow) {
    window
        .global::<ShortcutHelper>()
        .on_format_key(|ctrl, alt, shift, meta, text| {
            shortcut::format_key_combination(ctrl, alt, shift, meta, text.as_str())
                .map(SharedString::from)
                .unwrap_or_default()
        });
}

/// 加载配置并注册保存、导入、导出和更新回调。
fn initialize_settings_state(
    window: &SettingsWindow,
    runtime: &TaskRuntime,
    cancel: &CancellationToken,
) -> Result<(), SettingsError> {
    let config = config::load_or_create_config(None)?;
    snapin_foundation::i18n::set_language(config.general.language);
    apply_language(config.general.language);
    sync_config_to_window(window, &config);
    if let Err(error) = sync_hotkeys_to_gnome(&config.hotkeys) {
        tracing::warn!("进入设置中心同步系统全局快捷键失败: {error}");
    }
    let config_store = Arc::new(ConfigStore::new(config));
    callbacks::register(window, config_store.clone(), runtime.clone());
    update_controller::register_update_callbacks(
        window,
        runtime.clone(),
        cancel.clone(),
        config_store,
    );
    Ok(())
}

/// 等待在途保存任务收尾，避免退出路径与配置写入并发。
fn shutdown_settings_runtime(runtime: &TaskRuntime) {
    if !runtime.shutdown_blocking(Duration::from_secs(5)) {
        tracing::warn!("设置中心退出时仍有后台任务未结束，已按超时继续收尾");
    }
}

/// 设置窗口退出后，从磁盘同步最新的有效全局快捷键。
fn restore_latest_hotkeys() {
    if let Ok(latest_cfg) = config::load_or_create_config(None) {
        if let Err(e) = sync_hotkeys_to_gnome(&latest_cfg.hotkeys) {
            tracing::warn!("退出设置中心恢复系统全局快捷键失败: {e}");
        }
    }
}

/// 应用 Slint 打包翻译语言。
pub(crate) fn apply_language(language: snapin_config::config::UiLanguage) {
    if let Err(error) = slint::select_bundled_translation(language.slint_translation_code()) {
        tracing::warn!("切换界面语言失败，语言: {}，原因: {error}", language.code());
    }
}

/// 获取设置中心后台任务运行时：优先复用当前 Tokio 上下文，否则使用进程级共享实例。
pub(crate) fn settings_runtime() -> Result<TaskRuntime, SettingsError> {
    TaskRuntime::current(8).or_else(|current_error| {
        TaskRuntime::shared().cloned().ok_or_else(|| {
            SettingsError::Runtime(format!(
                "当前运行时不可用（{current_error}），进程级共享运行时创建失败"
            ))
        })
    })
}
