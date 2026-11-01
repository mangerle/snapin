//! Snapin 偏好设置中心图形界面模块（基于 Slint 声明式 GUI）。
//!
//! 采用独立轻量进程架构，提供现代化的卡片式设置面板，
//! 负责配置文件双向响应式同步、快捷键同步与系统集成。

slint::include_modules!();

mod config_store;
pub mod config_sync;
pub mod mapping;
pub mod shortcut;
pub mod update_controller;

use config_store::ConfigStore;
use config_sync::{extract_config_from_window, sync_config_to_window};

use slint::{ComponentHandle, SharedString};
use snapin_core::config::{self, Config};
use snapin_core::error::CoreError;
use snapin_daemon::notify_daemon_reload_config;
use snapin_platform::{
    get_platform_capabilities, open_path_in_file_manager, sync_autostart, sync_hotkeys_to_gnome,
    PlatformError,
};
use snapin_runtime::TaskRuntime;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
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

    /// 统一后台任务运行时初始化失败
    #[error("初始化设置中心后台任务运行时失败: {0}")]
    Runtime(String),
}

/// 运行 Slint 偏好设置中心图形界面
pub fn run_settings_gui() -> Result<(), SettingsError> {
    let window = SettingsWindow::new().map_err(|e| SettingsError::InitFailed(e.to_string()))?;

    // 设置 Wayland XDG 应用标识，使 Dock 栏与 Alt-Tab 匹配矢量图标
    #[cfg(target_os = "linux")]
    let _ = slint::set_xdg_app_id("snapin");

    window
        .window()
        .set_size(slint::LogicalSize::new(900.0, 620.0));
    window.set_app_version(SharedString::from(env!("CARGO_PKG_VERSION")));

    #[cfg(target_os = "linux")]
    let status_text = if get_platform_capabilities().gnome_extension_active {
        "GNOME Shell 窗口特权扩展已在线运行（贴图可强制置顶并精确落位）"
    } else {
        "GNOME Shell 窗口特权扩展未激活（贴图无法强制置顶与精确落位）"
    };
    #[cfg(not(target_os = "linux"))]
    let status_text = "系统原生置顶与热键驱动已就绪";

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

    let runtime = settings_runtime()?;
    let cancel = runtime.cancellation_token();
    let config = config::load_or_create_config(None)?;
    let config_store = Arc::new(ConfigStore::new(config.clone()));

    // 1. 同步初始配置到 Slint 界面并确保系统全局快捷键处于激活状态
    sync_config_to_window(&window, &config);
    if let Err(e) = sync_hotkeys_to_gnome(&config.hotkeys) {
        tracing::warn!("进入设置中心同步系统全局快捷键失败: {e}");
    }

    // 2. 注册界面交互与保存回调
    register_callbacks(&window, config_store.clone(), runtime.clone());
    update_controller::register_update_callbacks(
        &window,
        runtime.clone(),
        cancel.clone(),
        config_store.clone(),
    );

    let run_res = window
        .run()
        .map_err(|e| SettingsError::RuntimeFailed(e.to_string()));
    cancel.cancel();

    // 3. 等待在途保存任务收尾。保存与退出路径并发读写同一份 config.toml 时，
    // 直接读盘可能读到旧值并把它重新同步到 GNOME 快捷键。
    if !runtime.shutdown_blocking(Duration::from_secs(5)) {
        tracing::warn!("设置中心退出时仍有后台任务未结束，已按超时继续收尾");
    }

    // 4. 设置窗口退出后，从磁盘同步最新的有效全局快捷键
    if let Ok(latest_cfg) = config::load_or_create_config(None) {
        if let Err(e) = sync_hotkeys_to_gnome(&latest_cfg.hotkeys) {
            tracing::warn!("退出设置中心恢复系统全局快捷键失败: {e}");
        }
    }

    run_res?;
    Ok(())
}

/// 获取设置中心后台任务运行时：优先复用当前 Tokio 上下文，否则使用进程级共享实例。
fn settings_runtime() -> Result<TaskRuntime, SettingsError> {
    TaskRuntime::current(8).or_else(|current_error| {
        TaskRuntime::shared().cloned().ok_or_else(|| {
            SettingsError::Runtime(format!(
                "当前运行时不可用（{current_error}），进程级共享运行时创建失败"
            ))
        })
    })
}

/// 注册界面操作按钮回调事件
fn register_callbacks(win: &SettingsWindow, config_store: Arc<ConfigStore>, runtime: TaskRuntime) {
    let win_weak = win.as_weak();
    let cfg_clone = config_store.clone();
    let saving = Arc::new(AtomicBool::new(false));
    win.on_save_and_apply(move || {
        let Some(window) = win_weak.upgrade() else {
            return;
        };
        if saving.swap(true, Ordering::AcqRel) {
            window.set_status_message(SharedString::from("正在保存配置，请稍候..."));
            return;
        }

        // 始终以“最近一次成功保存”的快照为基底，再叠加界面字段。
        // 这样 UI 未暴露的 custom_update_url 等字段不会被默认值或空值覆盖。
        let mut updated = cfg_clone.snapshot();
        extract_config_from_window(&window, &mut updated);
        let sanitized = updated.sanitized();

        let config_path = match config::get_default_config_path() {
            Ok(path) => path,
            Err(error) => {
                saving.store(false, Ordering::Release);
                tracing::error!("获取配置文件路径失败: {error}");
                window.set_status_message(SharedString::from(format!(
                    "保存失败: 无法定位配置文件 ({error})"
                )));
                return;
            }
        };
        window.set_status_message(SharedString::from("正在保存配置并同步系统设置..."));

        let win_async = win_weak.clone();
        let config_for_task = sanitized;
        let cfg_after = cfg_clone.clone();
        let saving_after = saving.clone();
        let spawn_result = runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let mut warnings: Vec<String> = Vec::new();
                if let Err(error) = config::save_config(&config_for_task, &config_path) {
                    return Err(format!("保存配置失败: {error}"));
                }
                if let Err(error) = sync_hotkeys_to_gnome(&config_for_task.hotkeys) {
                    warnings.push(format!("系统快捷键同步失败 ({error})"));
                }
                if let Err(error) = sync_autostart(config_for_task.general.autostart) {
                    warnings.push(format!("开机自启动同步失败 ({error})"));
                }
                Ok((config_for_task, warnings))
            })
            .await;

            let _ = slint::invoke_from_event_loop(move || {
                saving_after.store(false, Ordering::Release);
                let Some(window) = win_async.upgrade() else {
                    return;
                };
                match result {
                    Ok(Ok((config, warnings))) => {
                        // 落盘成功后只提交一次新快照，并把收敛后的值回填界面，
                        // 避免“磁盘已收敛、内存与界面仍是脏值”的三方漂移。
                        cfg_after.commit(config.clone());
                        sync_config_to_window(&window, &config);
                        notify_daemon_reload_config();
                        if warnings.is_empty() {
                            window.set_status_message(SharedString::from(
                                "设置已成功保存并同步应用！",
                            ));
                        } else {
                            window.set_status_message(SharedString::from(format!(
                                "设置已保存，但{}",
                                warnings.join("；")
                            )));
                        }
                    }
                    Ok(Err(message)) => {
                        tracing::error!("{message}");
                        window.set_status_message(SharedString::from(message));
                    }
                    Err(error) => {
                        tracing::error!("设置保存任务异常: {error}");
                        window.set_status_message(SharedString::from("保存失败: 配置写入任务异常"));
                    }
                }
            });
        });

        if spawn_result.is_err() {
            saving.store(false, Ordering::Release);
            window.set_status_message(SharedString::from("保存失败: 无法提交后台任务"));
        }
    });

    let win_weak2 = win.as_weak();
    win.on_restore_defaults(move || {
        if let Some(w) = win_weak2.upgrade() {
            // 只改界面、不提前改内存快照；保存时仍以最近一次成功配置为基底，
            // UI 未暴露的字段因此不会被默认值静默重置。
            sync_config_to_window(&w, &Config::default());
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
