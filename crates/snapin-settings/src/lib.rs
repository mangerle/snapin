//! Snapin 偏好设置中心图形界面模块（基于 Slint 声明式 GUI）。
//!
//! 采用独立轻量进程架构，提供现代化的卡片式设置面板，
//! 负责配置文件双向响应式同步、快捷键同步与系统集成。

slint::include_modules!();

pub mod config_sync;
pub mod mapping;
pub mod shortcut;

use config_sync::{extract_config_from_window, sync_config_to_window};
use slint::{ComponentHandle, SharedString};
use snapin_core::config::{self, Config};
use snapin_core::error::CoreError;
use snapin_daemon::notify_daemon_reload_config;
use snapin_platform::{
    get_platform_capabilities, open_path_in_file_manager, suspend_hotkeys_for_settings,
    sync_autostart_desktop_file, sync_hotkeys_to_gnome, PlatformError,
};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// 偏好设置模块领域错误
#[derive(Error, Debug)]
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
}

/// 运行 Slint 偏好设置中心图形界面
pub fn run_settings_gui() -> Result<(), SettingsError> {
    let window = SettingsWindow::new().map_err(|e| SettingsError::InitFailed(e.to_string()))?;

    // 设置 Wayland XDG 应用标识，使 Dock 栏与 Alt-Tab 匹配矢量图标
    let _ = slint::set_xdg_app_id("snapin");

    window
        .window()
        .set_size(slint::LogicalSize::new(900.0, 620.0));
    window.set_app_version(SharedString::from(env!("CARGO_PKG_VERSION")));

    let caps = get_platform_capabilities();
    let status_text = if caps.gnome_extension_active {
        "GNOME Shell 原生特权扩展已在线运行"
    } else {
        "GNOME Shell 原生特权扩展未激活 (当前自动使用 XDG Portal)"
    };
    window.set_extension_status_text(SharedString::from(status_text));

    // 注册全局快捷键按键捕获与格式化回调
    let shortcut_helper = window.global::<ShortcutHelper>();
    shortcut_helper.on_format_key(|ctrl, alt, shift, meta, text| {
        let text_str = text.as_str();
        if let Some(formatted) = shortcut::format_key_combination(ctrl, alt, shift, meta, text_str)
        {
            SharedString::from(formatted)
        } else {
            SharedString::from("")
        }
    });

    let config = config::load_or_create_config(None)?;
    let config_arc = Arc::new(Mutex::new(config.clone()));

    // 1. 同步初始配置到 Slint 界面
    sync_config_to_window(&window, &config);

    // 2. 注册界面交互与保存回调
    register_callbacks(&window, config_arc);

    // 3. 临时挂起 Snapin 系统全局快捷键，确保录制时按键不被系统拦截触发
    if let Err(e) = suspend_hotkeys_for_settings() {
        tracing::warn!("进入设置中心临时挂起系统全局快捷键失败: {e}");
    }

    let run_res = window
        .run()
        .map_err(|e| SettingsError::RuntimeFailed(e.to_string()));

    // 4. 设置窗口退出后，从磁盘恢复最新的有效全局快捷键
    if let Ok(latest_cfg) = config::load_or_create_config(None) {
        if let Err(e) = sync_hotkeys_to_gnome(&latest_cfg.hotkeys) {
            tracing::warn!("退出设置中心恢复系统全局快捷键失败: {e}");
        }
    }

    run_res?;
    Ok(())
}

/// 注册界面操作按钮回调事件
fn register_callbacks(win: &SettingsWindow, config_arc: Arc<Mutex<Config>>) {
    let win_weak = win.as_weak();
    let cfg_clone = config_arc.clone();
    win.on_save_and_apply(move || {
        let Some(window) = win_weak.upgrade() else {
            return;
        };
        let Ok(mut config_guard) = cfg_clone.lock() else {
            window.set_status_message(SharedString::from("保存失败: 配置状态锁异常"));
            return;
        };

        extract_config_from_window(&window, &mut config_guard);

        let config_path = match config::get_default_config_path() {
            Ok(path) => path,
            Err(error) => {
                tracing::error!("获取配置文件路径失败: {error}");
                window.set_status_message(SharedString::from(format!(
                    "保存失败: 无法定位配置文件 ({error})"
                )));
                return;
            }
        };

        if let Err(error) = config::save_config(&config_guard, &config_path) {
            tracing::error!("保存配置文件失败: {error}");
            window.set_status_message(SharedString::from(format!("保存失败: {error}")));
            return;
        }

        let mut warnings: Vec<String> = Vec::new();
        if let Err(error) = sync_hotkeys_to_gnome(&config_guard.hotkeys) {
            tracing::warn!("保存配置后同步系统快捷键失败: {error}");
            warnings.push(format!("系统快捷键同步失败 ({error})"));
        }
        sync_autostart_desktop_file(config_guard.general.autostart);
        notify_daemon_reload_config();

        if warnings.is_empty() {
            window.set_status_message(SharedString::from("设置已成功保存并同步应用！"));
        } else {
            window.set_status_message(SharedString::from(format!(
                "设置已保存，但{}",
                warnings.join("；")
            )));
        }
    });

    let win_weak2 = win.as_weak();
    win.on_restore_defaults(move || {
        if let Some(w) = win_weak2.upgrade() {
            let default_cfg = Config::default();
            sync_config_to_window(&w, &default_cfg);
            if let Ok(mut cfg_guard) = config_arc.lock() {
                *cfg_guard = default_cfg;
            }
            w.set_status_message(SharedString::from("已还原为默认推荐配置（点击保存后生效）"));
        }
    });

    win.on_open_config_folder(|| {
        if let Ok(cfg_path) = config::get_default_config_path() {
            if let Some(parent) = cfg_path.parent() {
                open_path_in_file_manager(parent);
            }
        }
    });
}
