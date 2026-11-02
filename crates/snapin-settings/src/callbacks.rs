//! 设置中心保存、导入、导出与恢复默认值回调。
//!
//! Slint 回调只负责收集意图并提交后台任务；配置读写、系统快捷键和自启动同步
//! 统一通过 `TaskRuntime` 的可等待阻塞入口执行，避免在 UI 线程上做同步 I/O。

use super::*;
use snapin_runtime::TaskPool;

/// 导入配置文件允许的最大字节数。
const CONFIG_IMPORT_LIMIT: usize = 1024 * 1024;

/// 保存、导入和导出回调共享的配置与运行时上下文。
#[derive(Clone)]
struct CallbackContext {
    config_store: Arc<ConfigStore>,
    runtime: TaskRuntime,
    saving: Arc<AtomicBool>,
}

/// 注册界面操作按钮回调事件。
pub(crate) fn register(win: &SettingsWindow, config_store: Arc<ConfigStore>, runtime: TaskRuntime) {
    let context = CallbackContext {
        config_store,
        runtime,
        saving: Arc::new(AtomicBool::new(false)),
    };
    register_save(win, context.clone());
    register_import(win, context.clone());
    register_export(win, context.clone());
    register_misc(win, context);
}

/// 注册“保存并应用”回调。
fn register_save(win: &SettingsWindow, context: CallbackContext) {
    let weak = win.as_weak();
    win.on_save_and_apply(move || handle_save_click(&weak, &context));
}

/// 开始一次保存：先取得界面快照，成功后提交后台 I/O。
fn handle_save_click(weak: &slint::Weak<SettingsWindow>, context: &CallbackContext) {
    let Some(window) = weak.upgrade() else {
        return;
    };
    if context.saving.swap(true, Ordering::AcqRel) {
        window.set_status_message(SharedString::from(snapin_foundation::i18n::tr(
            "正在保存配置，请稍候...",
        )));
        return;
    }
    let Some((config, path)) = prepare_save_request(&window, context) else {
        context.saving.store(false, Ordering::Release);
        return;
    };
    window.set_status_message(SharedString::from(snapin_foundation::i18n::tr(
        "正在保存配置并同步系统设置...",
    )));
    spawn_save_task(weak.clone(), context.clone(), config, path);
}

/// 以最近一次成功保存的快照为基底叠加界面字段，并定位默认配置路径。
fn prepare_save_request(
    window: &SettingsWindow,
    context: &CallbackContext,
) -> Option<(Config, std::path::PathBuf)> {
    let mut config = context.config_store.snapshot();
    extract_config_from_window(window, &mut config);
    let config = config.sanitized();
    match config::get_default_config_path() {
        Ok(path) => Some((config, path)),
        Err(error) => {
            tracing::warn!("获取配置文件路径失败: {error}");
            window.set_status_message(SharedString::from(format!(
                "保存失败: 无法定位配置文件 ({error})"
            )));
            None
        }
    }
}

/// 提交保存任务，并在完成后把结果投递回 Slint 事件循环。
fn spawn_save_task(
    weak: slint::Weak<SettingsWindow>,
    context: CallbackContext,
    config: Config,
    path: std::path::PathBuf,
) {
    let task_runtime = context.runtime.clone();
    let finish_context = context.clone();
    let error_weak = weak.clone();
    let spawn_result = context.runtime.spawn(async move {
        let result = task_runtime
            .run_blocking_in(TaskPool::Io, move || save_config_and_sync(config, path))
            .await
            .unwrap_or_else(|error| Err(format!("配置写入任务异常: {error}")));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = weak.upgrade() {
                finish_save(&window, &finish_context, result);
            }
        });
    });
    if let Err(error) = spawn_result {
        context.saving.store(false, Ordering::Release);
        tracing::error!("提交配置保存任务失败: {error}");
        if let Some(window) = error_weak.upgrade() {
            window.set_status_message(SharedString::from(snapin_foundation::i18n::tr(
                "保存失败: 无法提交后台任务",
            )));
        }
    }
}

/// 保存配置并尽力同步系统快捷键与自启动；系统同步失败只形成警告。
fn save_config_and_sync(
    config: Config,
    path: std::path::PathBuf,
) -> Result<(Config, Vec<String>), String> {
    config::save_config(&config, &path).map_err(|error| format!("保存配置失败: {error}"))?;
    let warnings = sync_system_settings(&config);
    Ok((config, warnings))
}

/// 回填保存结果，提交新配置快照并更新界面提示。
fn finish_save(
    window: &SettingsWindow,
    context: &CallbackContext,
    result: Result<(Config, Vec<String>), String>,
) {
    context.saving.store(false, Ordering::Release);
    match result {
        Ok((config, warnings)) => {
            context.config_store.commit(config.clone());
            sync_config_to_window(window, &config);
            notify_daemon_reload_config();
            let message = if warnings.is_empty() {
                "设置已成功保存并同步应用！".to_string()
            } else {
                format!("设置已保存，但{}", warnings.join("；"))
            };
            window.set_status_message(SharedString::from(message));
        }
        Err(message) => {
            tracing::warn!("{message}");
            window.set_status_message(SharedString::from(message));
        }
    }
}

/// 注册“导入配置”回调。
fn register_import(win: &SettingsWindow, context: CallbackContext) {
    let weak = win.as_weak();
    win.on_import_config(move || handle_import_click(&weak, &context));
}

/// 开始一次配置导入。
fn handle_import_click(weak: &slint::Weak<SettingsWindow>, context: &CallbackContext) {
    let Some(window) = weak.upgrade() else {
        return;
    };
    if context.saving.swap(true, Ordering::AcqRel) {
        window.set_status_message(SharedString::from(snapin_foundation::i18n::tr(
            "正在处理配置，请稍候...",
        )));
        return;
    }
    window.set_status_message(SharedString::from(snapin_foundation::i18n::tr(
        "正在选择要导入的配置文件...",
    )));
    spawn_import_task(weak.clone(), context.clone());
}

/// 选择文件后在阻塞池中完成校验、保存与系统同步。
fn spawn_import_task(weak: slint::Weak<SettingsWindow>, context: CallbackContext) {
    let task_runtime = context.runtime.clone();
    let finish_context = context.clone();
    let error_weak = weak.clone();
    let spawn_result = context.runtime.spawn(async move {
        let result = match pick_config_import_path().await {
            Ok(Some(path)) => task_runtime
                .run_blocking_in(TaskPool::Io, move || import_config_file(&path))
                .await
                .unwrap_or_else(|error| Err(format!("配置导入任务异常: {error}"))),
            Ok(None) => Err("已取消配置导入".to_string()),
            Err(error) => Err(format!("打开配置导入对话框失败: {error}")),
        };
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = weak.upgrade() {
                finish_import(&window, &finish_context, result);
            }
        });
    });
    if let Err(error) = spawn_result {
        context.saving.store(false, Ordering::Release);
        tracing::error!("提交配置导入任务失败: {error}");
        if let Some(window) = error_weak.upgrade() {
            window.set_status_message(SharedString::from(snapin_foundation::i18n::tr(
                "配置导入失败: 无法提交后台任务",
            )));
        }
    }
}

/// 读取、限长解析并保存导入配置。
fn import_config_file(path: &std::path::Path) -> Result<(Config, Vec<String>), String> {
    let content = snapin_foundation::read_utf8_file_limited(path, CONFIG_IMPORT_LIMIT)
        .map_err(|error| format!("读取配置文件失败: {error}"))?;
    let imported = config::parse_imported_config(&content, path)
        .map_err(|error| format!("配置文件校验失败: {error}"))?;
    let target = config::get_default_config_path()
        .map_err(|error| format!("定位默认配置路径失败: {error}"))?;
    config::save_config(&imported, &target)
        .map_err(|error| format!("保存导入配置失败: {error}"))?;
    let warnings = sync_system_settings(&imported);
    Ok((imported, warnings))
}

/// 回填导入结果，更新语言、配置快照和界面。
fn finish_import(
    window: &SettingsWindow,
    context: &CallbackContext,
    result: Result<(Config, Vec<String>), String>,
) {
    context.saving.store(false, Ordering::Release);
    match result {
        Ok((config, warnings)) => {
            context.config_store.commit(config.clone());
            snapin_foundation::i18n::set_language(config.general.language);
            apply_language(config.general.language);
            sync_config_to_window(window, &config);
            notify_daemon_reload_config();
            let message = if warnings.is_empty() {
                "配置导入成功并已应用".to_string()
            } else {
                format!("配置已导入，但{}", warnings.join("；"))
            };
            window.set_status_message(SharedString::from(message));
        }
        Err(message) => {
            tracing::warn!("{message}");
            window.set_status_message(SharedString::from(message));
        }
    }
}

/// 注册“导出配置”回调。
fn register_export(win: &SettingsWindow, context: CallbackContext) {
    let weak = win.as_weak();
    win.on_export_config(move || handle_export_click(&weak, &context));
}

/// 开始一次配置导出。
fn handle_export_click(weak: &slint::Weak<SettingsWindow>, context: &CallbackContext) {
    let Some(window) = weak.upgrade() else {
        return;
    };
    let mut config = context.config_store.snapshot();
    extract_config_from_window(&window, &mut config);
    let config = config.sanitized();
    window.set_status_message(SharedString::from(snapin_foundation::i18n::tr(
        "正在选择配置导出位置...",
    )));
    spawn_export_task(weak.clone(), context.runtime.clone(), config);
}

/// 选择目标文件后在阻塞池中写出配置。
fn spawn_export_task(weak: slint::Weak<SettingsWindow>, runtime: TaskRuntime, config: Config) {
    let task_runtime = runtime.clone();
    let error_weak = weak.clone();
    let spawn_result = runtime.spawn(async move {
        let result = match pick_config_export_path().await {
            Ok(Some(mut path)) => {
                if path.extension().is_none() {
                    path.set_extension("toml");
                }
                task_runtime
                    .run_blocking_in(TaskPool::Io, move || {
                        config::save_config(&config, &path)
                            .map(|()| path)
                            .map_err(|error| format!("导出配置失败: {error}"))
                    })
                    .await
                    .unwrap_or_else(|error| Err(format!("配置导出任务异常: {error}")))
            }
            Ok(None) => Err("已取消配置导出".to_string()),
            Err(error) => Err(format!("打开配置导出对话框失败: {error}")),
        };
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = weak.upgrade() {
                let message = match result {
                    Ok(path) => format!("配置已导出至: {}", path.display()),
                    Err(message) => message,
                };
                window.set_status_message(SharedString::from(message));
            }
        });
    });
    if let Err(error) = spawn_result {
        tracing::error!("提交配置导出任务失败: {error}");
        if let Some(window) = error_weak.upgrade() {
            window.set_status_message(SharedString::from(snapin_foundation::i18n::tr(
                "配置导出失败: 无法提交后台任务",
            )));
        }
    }
}

/// 注册恢复默认值、打开配置目录等同步界面动作。
fn register_misc(win: &SettingsWindow, _context: CallbackContext) {
    let weak = win.as_weak();
    win.on_restore_defaults(move || {
        if let Some(window) = weak.upgrade() {
            sync_config_to_window(&window, &Config::default());
            window.set_status_message(SharedString::from(snapin_foundation::i18n::tr(
                "已还原为默认推荐配置（点击保存后生效）",
            )));
        }
    });
    win.on_open_config_folder(|| {
        if let Ok(path) = config::get_default_config_path() {
            if let Some(parent) = path.parent() {
                open_path_in_file_manager(parent);
            }
        }
    });
}

/// 同步系统快捷键与自启动，返回可展示的非致命警告。
fn sync_system_settings(config: &Config) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Err(error) = sync_hotkeys_to_gnome(&config.hotkeys) {
        warnings.push(format!("系统快捷键同步失败 ({error})"));
    }
    if let Err(error) = sync_autostart(config.general.autostart) {
        warnings.push(format!("开机自启动同步失败 ({error})"));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_rejects_oversized_config_file() {
        let dir = tempfile::tempdir().expect("应可创建临时目录");
        let path = dir.path().join("oversized.toml");
        std::fs::write(&path, vec![b' '; CONFIG_IMPORT_LIMIT.saturating_add(1)])
            .expect("应可写入超限配置");

        let error = import_config_file(&path).expect_err("超限配置必须被拒绝");
        assert!(error.contains("读取上限"));
    }
}
