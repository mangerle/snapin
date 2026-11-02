//! 剪贴板历史独立面板。

use super::*;
use snapin_runtime::TaskPool;

/// 打开剪贴板历史独立面板。
///
/// # Errors
///
/// 窗口创建、历史读取、运行时初始化或事件循环运行失败时返回 [`SettingsError`]。
pub fn run_clipboard_history_panel() -> Result<(), SettingsError> {
    let window = create_clipboard_panel()?;
    initialize_clipboard_language();
    let entries = Arc::new(load_history_entries()?);
    apply_clipboard_rows(&window, &entries);
    let runtime = settings_runtime()?;
    register_clipboard_callbacks(&window, entries, runtime.clone());
    let run_result = window
        .run()
        .map_err(|error| SettingsError::RuntimeFailed(error.to_string()));
    runtime.shutdown_blocking(Duration::from_secs(3));
    run_result
}

/// 创建剪贴板历史面板窗口。
fn create_clipboard_panel() -> Result<ClipboardPanel, SettingsError> {
    ClipboardPanel::new().map_err(|error| SettingsError::InitFailed(error.to_string()))
}

/// 按用户配置应用面板语言。
fn initialize_clipboard_language() {
    if let Ok(config) = config::load_or_create_config(None) {
        snapin_foundation::i18n::set_language(config.general.language);
        apply_language(config.general.language);
    }
}

/// 读取剪贴板历史并提供明确的设置中心错误。
fn load_history_entries() -> Result<Vec<ClipboardEntry>, SettingsError> {
    load_clipboard_history().map_err(|error| SettingsError::Runtime(error.to_string()))
}

/// 注册激活、清空与关闭面板回调。
fn register_clipboard_callbacks(
    window: &ClipboardPanel,
    entries: Arc<Vec<ClipboardEntry>>,
    runtime: TaskRuntime,
) {
    register_activate_callback(window, entries, runtime.clone());
    register_clear_callback(window, runtime);
    register_close_callback(window);
}

/// 注册条目激活回调：写入剪贴板后请求守护进程创建贴图。
fn register_activate_callback(
    window: &ClipboardPanel,
    entries: Arc<Vec<ClipboardEntry>>,
    runtime: TaskRuntime,
) {
    let weak = window.as_weak();
    window.on_activate(move |index| {
        let Some(entry) = entries.get(index as usize).cloned() else {
            return;
        };
        let Some(window) = weak.upgrade() else {
            return;
        };
        window.set_status_message(SharedString::from("正在读取剪贴板历史..."));
        spawn_activate_task(weak.clone(), runtime.clone(), entry);
    });
}

/// 在 I/O 池读取历史内容并写入剪贴板，成功后再请求守护进程创建贴图。
fn spawn_activate_task(
    weak: slint::Weak<ClipboardPanel>,
    runtime: TaskRuntime,
    entry: ClipboardEntry,
) {
    let task_runtime = runtime.clone();
    let error_weak = weak.clone();
    let spawn_result = runtime.spawn(async move {
        let copied = task_runtime
            .run_blocking_in(TaskPool::Io, move || copy_entry_to_clipboard(&entry))
            .await
            .unwrap_or_else(|error| Err(format!("复制剪贴板历史任务异常: {error}")));
        let message = match copied {
            Err(message) => message,
            Ok(()) => {
                match snapin_daemon::send_command_to_daemon(&snapin_daemon::DaemonRequest::Paste)
                    .await
                {
                    Ok(true) => "已复制到剪贴板，正在创建贴图".to_string(),
                    Ok(false) => "已复制到剪贴板，但守护进程未响应贴图请求".to_string(),
                    Err(error) => format!("已复制到剪贴板，但创建贴图失败: {error}"),
                }
            }
        };
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = weak.upgrade() {
                window.set_status_message(SharedString::from(message));
            }
        });
    });
    if let Err(error) = spawn_result {
        if let Some(window) = error_weak.upgrade() {
            window.set_status_message(SharedString::from(format!("无法提交复制任务: {error}")));
        }
    }
}

/// 将一条历史记录写回系统剪贴板。
fn copy_entry_to_clipboard(entry: &ClipboardEntry) -> Result<(), String> {
    match entry.kind {
        ClipboardEntryKind::Text | ClipboardEntryKind::Color => entry
            .text
            .as_deref()
            .ok_or_else(|| "文本历史缺少内容".to_string())
            .and_then(|text| write_text_to_clipboard(text).map_err(|error| error.to_string())),
        ClipboardEntryKind::Image => load_clipboard_image(entry)
            .map_err(|error| error.to_string())
            .and_then(|image| write_image_to_clipboard(&image).map_err(|error| error.to_string())),
    }
}

/// 注册清空历史回调。
fn register_clear_callback(window: &ClipboardPanel, runtime: TaskRuntime) {
    let weak = window.as_weak();
    window.on_clear_history(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if let Err(error) = spawn_clear_task(weak.clone(), runtime.clone()) {
            window.set_status_message(SharedString::from(format!("无法提交清空任务: {error}")));
        }
    });
}

/// 在线程池删除历史目录，成功后再清空 Slint 模型。
fn spawn_clear_task(
    weak: slint::Weak<ClipboardPanel>,
    runtime: TaskRuntime,
) -> Result<(), snapin_runtime::TaskRuntimeError> {
    let task_runtime = runtime.clone();
    runtime
        .spawn(async move {
            let result = task_runtime
                .run_blocking_in(TaskPool::Io, clear_clipboard_history)
                .await
                .unwrap_or_else(|error| {
                    Err(snapin_storage::StorageError::Io {
                        path: "剪贴板历史清理任务".to_string(),
                        source: std::io::Error::other(error.to_string()),
                    })
                });
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = weak.upgrade() {
                    match result {
                        Ok(()) => {
                            apply_clipboard_rows(&window, &[]);
                            window.set_status_message(SharedString::from(
                                snapin_foundation::i18n::tr("剪贴板历史已清空"),
                            ));
                        }
                        Err(error) => window.set_status_message(SharedString::from(format!(
                            "清空剪贴板历史失败: {error}"
                        ))),
                    }
                }
            });
        })
        .map(|_| ())
}

/// 注册关闭面板回调。
fn register_close_callback(window: &ClipboardPanel) {
    let weak = window.as_weak();
    window.on_close_panel(move || {
        if let Some(window) = weak.upgrade() {
            let _ = window.hide();
        }
    });
}

fn apply_clipboard_rows(window: &ClipboardPanel, entries: &[ClipboardEntry]) {
    let mut titles = Vec::with_capacity(entries.len());
    let mut previews = Vec::with_capacity(entries.len());
    for entry in entries {
        let (title, preview) = match entry.kind {
            ClipboardEntryKind::Text => (
                "文本",
                entry
                    .text
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .take(120)
                    .collect::<String>(),
            ),
            ClipboardEntryKind::Color => ("颜色", entry.text.clone().unwrap_or_default()),
            ClipboardEntryKind::Image => ("图像", entry.image_file.clone().unwrap_or_default()),
        };
        titles.push(SharedString::from(title));
        previews.push(SharedString::from(preview));
    }
    window.set_titles(ModelRc::new(VecModel::from(titles)));
    window.set_previews(ModelRc::new(VecModel::from(previews)));
}
