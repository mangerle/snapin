//! 剪贴板历史独立面板。

use super::*;
use snapin_daemon::{
    send_request_to_path, DaemonErrorCode, DaemonRequest, DaemonResponse, IpcInvocation,
    IpcListener,
};
use snapin_foundation::AppPaths;
use snapin_runtime::TaskPool;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{mpsc, watch};

/// 剪贴板控制通道容量；面板关闭请求极少，保留小队列即可提供明确背压。
const CONTROL_CHANNEL_CAPACITY: usize = 4;
/// 等待控制监听器完成绑定的上限。
const CONTROL_START_TIMEOUT: Duration = Duration::from_secs(2);
/// 重复启动时等待控制套接字就绪的重试次数。
const CONTROL_RETRY_ATTEMPTS: usize = 20;
/// 控制套接字启动与关闭请求之间的重试间隔。
const CONTROL_RETRY_DELAY: Duration = Duration::from_millis(50);
/// 与单实例锁分离的控制套接字文件名，避免两套文件锁互相竞争。
const CONTROL_SOCKET_NAME: &str = "clipboard-panel-control.sock";

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
    let control = ClipboardPanelControl::start(&runtime)?;
    register_clipboard_callbacks(&window, entries, runtime.clone());
    let run_result = window.run().map_err(|error| SettingsError::Runtime {
        component: "剪贴板历史面板事件循环",
        reason: error.to_string(),
    });
    control.stop();
    runtime.shutdown_blocking(Duration::from_secs(3));
    run_result
}

/// 请求正在运行的剪贴板历史面板退出。
///
/// 返回 `Ok(true)` 表示面板已确认关闭请求；返回 `Ok(false)` 表示单实例锁存在，
/// 但控制通道在等待窗口内仍未就绪。调用方应只记录诊断信息并正常退出，
/// 避免热键连续触发时堆积错误进程。
///
/// # Errors
///
/// 运行时目录不可用、IPC 通信失败或面板返回明确业务错误时返回 [`SettingsError`]。
pub async fn request_clipboard_history_panel_close() -> Result<bool, SettingsError> {
    let socket_path = control_socket_path()?;
    for attempt in 0..CONTROL_RETRY_ATTEMPTS {
        match send_request_to_path(&socket_path, &DaemonRequest::CloseClipboardPanel).await {
            Ok(Some(DaemonResponse::Ok)) => return Ok(true),
            Ok(None) => {}
            Ok(Some(DaemonResponse::Error(failure))) => {
                return Err(control_error("关闭请求被拒绝", failure.message));
            }
            Ok(Some(response)) => {
                return Err(control_error(
                    "关闭请求收到非预期响应",
                    format!("{response:?}"),
                ));
            }
            Err(error) => return Err(control_error("发送关闭请求失败", error.to_string())),
        }
        if attempt + 1 < CONTROL_RETRY_ATTEMPTS {
            tokio::time::sleep(CONTROL_RETRY_DELAY).await;
        }
    }
    Ok(false)
}

/// 面板进程内持有的控制监听器生命周期句柄。
struct ClipboardPanelControl {
    shutdown: watch::Sender<bool>,
}

impl ClipboardPanelControl {
    /// 启动剪贴板面板控制监听器，并等待套接字进入可连接状态。
    fn start(runtime: &TaskRuntime) -> Result<Self, SettingsError> {
        let socket_path = control_socket_path()?;
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
        let (shutdown_sender, shutdown_receiver) = watch::channel(false);
        let (command_sender, mut command_receiver) = mpsc::channel(CONTROL_CHANNEL_CAPACITY);

        runtime
            .spawn(async move {
                while let Some(invocation) = command_receiver.recv().await {
                    handle_control_invocation(invocation);
                }
            })
            .map_err(|error| control_error("启动控制命令处理器失败", error.to_string()))?;

        runtime
            .spawn(async move {
                match IpcListener::bind(Some(socket_path)).await {
                    Ok(listener) => {
                        let _ = ready_sender.send(Ok(()));
                        listener.run_loop(command_sender, shutdown_receiver).await;
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error.to_string()));
                    }
                }
            })
            .map_err(|error| control_error("提交控制监听器任务失败", error.to_string()))?;

        match ready_receiver.recv_timeout(CONTROL_START_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                shutdown: shutdown_sender,
            }),
            Ok(Err(reason)) => Err(control_error("控制监听器绑定失败", reason)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(control_error(
                "控制监听器启动超时",
                "等待套接字就绪超过 2 秒",
            )),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(control_error(
                "控制监听器启动失败",
                "监听任务在报告就绪前退出",
            )),
        }
    }

    /// 通知控制监听循环退出。
    fn stop(&self) {
        let _ = self.shutdown.send(true);
    }
}

/// 处理面板控制通道请求；仅关闭请求会触发事件循环退出。
fn handle_control_invocation(invocation: IpcInvocation) {
    let IpcInvocation {
        request,
        responder,
        cancelled: _,
    } = invocation;
    let should_close = matches!(&request, DaemonRequest::CloseClipboardPanel);
    let response = match request {
        DaemonRequest::CloseClipboardPanel => DaemonResponse::Ok,
        DaemonRequest::Ping => DaemonResponse::Pong,
        other => DaemonResponse::error(
            DaemonErrorCode::Unsupported,
            format!("剪贴板历史面板控制通道不支持请求: {other:?}"),
        ),
    };
    let _ = responder.send(response);
    if should_close {
        schedule_clipboard_panel_exit();
    }
}

/// 将退出动作投递到 Slint 主事件循环。
fn schedule_clipboard_panel_exit() {
    if let Err(error) = slint::invoke_from_event_loop(quit_clipboard_panel_event_loop) {
        tracing::warn!("投递剪贴板历史面板退出事件失败: {error}");
    }
}

/// 从 Slint 事件循环回调中结束面板进程。
fn quit_clipboard_panel_event_loop() {
    if let Err(error) = slint::quit_event_loop() {
        tracing::warn!("结束剪贴板历史面板事件循环失败: {error}");
    }
}

/// 获取控制套接字路径并确保私有运行时目录存在。
fn control_socket_path() -> Result<PathBuf, SettingsError> {
    AppPaths::ensure_runtime_dir().map_err(|error| {
        control_error("创建剪贴板历史面板控制套接字目录失败", error.to_string())
    })?;
    Ok(AppPaths::runtime_dir().join(CONTROL_SOCKET_NAME))
}

/// 构造控制通道领域错误，统一补充通信阶段上下文。
fn control_error(stage: &'static str, reason: impl Into<String>) -> SettingsError {
    SettingsError::Runtime {
        component: "剪贴板历史面板控制通道",
        reason: format!("{stage}: {}", reason.into()),
    }
}

/// 创建剪贴板历史面板窗口。
fn create_clipboard_panel() -> Result<ClipboardPanel, SettingsError> {
    ClipboardPanel::new().map_err(|error| SettingsError::Initialization {
        component: "剪贴板历史面板",
        reason: error.to_string(),
    })
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
    load_clipboard_history().map_err(|error| SettingsError::Runtime {
        component: "剪贴板历史数据加载",
        reason: error.to_string(),
    })
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
    let activating = Arc::new(AtomicBool::new(false));
    window.on_activate(move |index| {
        let Some(entry) = entries.get(index as usize).cloned() else {
            return;
        };
        let Some(window) = weak.upgrade() else {
            return;
        };
        if activating.swap(true, Ordering::AcqRel) {
            window.set_status_message(SharedString::from("正在复制上一条历史，请稍候..."));
            return;
        }
        window.set_status_message(SharedString::from("正在读取剪贴板历史..."));
        spawn_activate_task(weak.clone(), runtime.clone(), entry, activating.clone());
    });
}

struct ActivationGuard(Arc<AtomicBool>);

impl Drop for ActivationGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// 在 I/O 池读取历史内容并写入剪贴板，成功后再请求守护进程创建贴图。
fn spawn_activate_task(
    weak: slint::Weak<ClipboardPanel>,
    runtime: TaskRuntime,
    entry: ClipboardEntry,
    activating: Arc<AtomicBool>,
) {
    let task_runtime = runtime.clone();
    let error_weak = weak.clone();
    let task_activating = activating.clone();
    let spawn_result = runtime.spawn(async move {
        let _guard = ActivationGuard(task_activating);
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
        activating.store(false, Ordering::Release);
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
    window.on_close_panel(quit_clipboard_panel_event_loop);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activation_guard_releases_flag_on_drop() {
        let flag = Arc::new(AtomicBool::new(true));
        {
            let _guard = ActivationGuard(flag.clone());
        }
        assert!(!flag.load(Ordering::Acquire));
    }
}
