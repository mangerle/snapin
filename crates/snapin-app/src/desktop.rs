//! 桌面副作用端口实现：选区缓存、剪贴板、文件管理器与桌面通知。
//!
//! # 设计原理
//!
//! 表现层只通过 [`DesktopPort`] 发起用户动作，本模块把调用转交给
//! `snapin-platform`，并把平台错误转换为统一的 [`PortError`]。剪贴板读写
//! 必须经过有界后台队列并携带完成回调：GUI 线程既不等待系统剪贴板服务，
//! 也不会在真实写入完成前错误地向用户报告成功。
//!
//! 桌面通知统一提交到应用级 [`TaskRuntime`]，避免在 winit 事件循环中阻塞等待
//! D-Bus，也不再为每条通知创建线程。
//!
//! # 代价与权衡
//!
//! 剪贴板任务串行执行，保证同一进程内的写入顺序稳定；桌面通知与媒体任务共享
//! 同一运行时的并发名额，通知属于非关键提示，提交失败时只记录警告。

use crate::config::{config_snapshot, SharedConfig};
use arc_swap::ArcSwap;
use snapin_platform::{
    open_path_in_file_manager, open_url, read_clipboard_image, read_clipboard_text,
    send_desktop_notification, write_image_to_clipboard, write_text_to_clipboard,
};
use snapin_runtime::{TaskPool, TaskRuntime};
use snapin_storage::{load_last_selection, save_last_selection};
use snapin_ui::snipper::Rect;
use snapin_ui::{
    ClipboardJobId, ClipboardPayload, ClipboardResult, ClipboardSink, ClipboardWrite, DesktopPort,
    PortError, PortErrorKind,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::{self, error::TrySendError};

/// 桌面写操作队列容量；按用户交互频率设置上限，满队列时快速失败。
const DESKTOP_WRITE_QUEUE_CAPACITY: usize = 8;

/// 一次串行执行的剪贴板任务。
struct ClipboardTask {
    id: ClipboardJobId,
    request: Option<ClipboardRequest>,
    sink: Option<ClipboardSink>,
}

/// 剪贴板后台请求。
enum ClipboardRequest {
    /// 将用户内容写入系统剪贴板。
    Write(ClipboardWrite),
    /// 按“图像优先、文本兜底”的顺序读取当前系统剪贴板。
    Read,
}

impl ClipboardTask {
    /// 在阻塞线程中执行请求，并向唯一等待方回传真实结果。
    fn execute(mut self) {
        let request = self.request.take().expect("剪贴板任务请求在执行前必须存在");
        let result = match request {
            ClipboardRequest::Write(ClipboardWrite::Text(text)) => write_text_to_clipboard(&text)
                .map(|()| ClipboardResult::Written)
                .map_err(|error| clipboard_error("写入系统剪贴板文本", error)),
            ClipboardRequest::Write(ClipboardWrite::Image(image)) => {
                write_image_to_clipboard(&image)
                    .map(|()| ClipboardResult::Written)
                    .map_err(|error| clipboard_error("写入系统剪贴板图像", error))
            }
            ClipboardRequest::Read => read_current_clipboard(),
        };
        self.complete(result);
    }

    /// 任务未被队列接收时解除析构兜底，避免宿主已拒绝提交却回传伪完成事件。
    fn disarm(&mut self) {
        self.request = None;
        self.sink = None;
    }

    /// 一次性完成回调；重复调用和正常完成后析构均为无操作。
    fn complete(&mut self, result: Result<ClipboardResult, PortError>) {
        if let Some(sink) = self.sink.take() {
            sink(self.id, result);
        }
    }
}

impl Drop for ClipboardTask {
    fn drop(&mut self) {
        self.complete(Err(PortError::with_kind(
            PortErrorKind::Other,
            "执行系统剪贴板任务",
            "任务在返回结果前异常退出",
        )));
    }
}

/// 读取当前系统剪贴板，优先图像，其次文本。
fn read_current_clipboard() -> Result<ClipboardResult, PortError> {
    match read_clipboard_image() {
        Ok(Some(image)) => {
            return Ok(ClipboardResult::Payload(ClipboardPayload::Image(Arc::new(
                image,
            ))))
        }
        Ok(None) => {}
        Err(error) => return Err(clipboard_error("读取系统剪贴板图像", error)),
    }

    match read_clipboard_text() {
        Ok(Some(text)) => Ok(ClipboardResult::Payload(ClipboardPayload::Text(text))),
        Ok(None) => Ok(ClipboardResult::Payload(ClipboardPayload::Empty)),
        Err(error) => Err(clipboard_error("读取系统剪贴板文本", error)),
    }
}

/// 将平台错误转换为稳定 UI 端口错误，避免泄漏具体平台错误类型。
fn clipboard_error(operation: &str, error: impl std::fmt::Display) -> PortError {
    PortError::with_kind(PortErrorKind::Unavailable, operation, error.to_string())
}

/// 桌面副作用端口实现。
pub(crate) struct AppDesktop {
    config: SharedConfig,
    runtime: TaskRuntime,
    last_selection: ArcSwap<Option<Rect>>,
    clipboard_tx: mpsc::Sender<ClipboardTask>,
    next_clipboard_job_id: AtomicU64,
    selection_tx: mpsc::Sender<Rect>,
}

impl AppDesktop {
    /// 构建桌面端口。
    pub(crate) fn new(config: SharedConfig, runtime: TaskRuntime) -> Result<Self, PortError> {
        let last_selection = load_last_selection().map(|(x, y, width, height)| Rect {
            x,
            y,
            width,
            height,
        });
        let (clipboard_tx, clipboard_rx) = mpsc::channel(DESKTOP_WRITE_QUEUE_CAPACITY);
        let (selection_tx, selection_rx) = mpsc::channel(DESKTOP_WRITE_QUEUE_CAPACITY);
        let cancel = runtime.cancellation_token();
        runtime
            .spawn_in(
                TaskPool::Background,
                run_clipboard_worker(clipboard_rx, cancel.clone()),
            )
            .map_err(|error| worker_start_error("启动系统剪贴板任务", error))?;
        runtime
            .spawn_in(
                TaskPool::Background,
                run_selection_writer(selection_rx, cancel),
            )
            .map_err(|error| worker_start_error("启动历史选区写入任务", error))?;
        Ok(Self {
            config,
            runtime,
            last_selection: ArcSwap::from_pointee(last_selection),
            clipboard_tx,
            next_clipboard_job_id: AtomicU64::new(1),
            selection_tx,
        })
    }

    /// 提交一个剪贴板任务并返回稳定任务标识。
    fn submit_clipboard(
        &self,
        request: ClipboardRequest,
        sink: ClipboardSink,
    ) -> Result<ClipboardJobId, PortError> {
        let id = ClipboardJobId::new(self.next_clipboard_job_id.fetch_add(1, Ordering::Relaxed));
        let task = ClipboardTask {
            id,
            request: Some(request),
            sink: Some(sink),
        };
        match self.clipboard_tx.try_send(task) {
            Ok(()) => Ok(id),
            Err(TrySendError::Full(mut task)) => {
                task.disarm();
                Err(PortError::with_kind(
                    PortErrorKind::Busy,
                    "提交系统剪贴板任务",
                    "桌面任务队列已满",
                ))
            }
            Err(TrySendError::Closed(mut task)) => {
                task.disarm();
                Err(PortError::with_kind(
                    PortErrorKind::Unavailable,
                    "提交系统剪贴板任务",
                    "桌面任务已停止",
                ))
            }
        }
    }
}

/// 串行消费剪贴板任务；停机时排空队列后再退出。
async fn run_clipboard_worker(
    mut receiver: mpsc::Receiver<ClipboardTask>,
    cancel: snapin_runtime::CancellationToken,
) {
    loop {
        let task = tokio::select! {
            _ = cancel.cancelled() => None,
            task = receiver.recv() => task,
        };
        if let Some(task) = task {
            execute_clipboard_task(task).await;
            continue;
        }
        while let Ok(task) = receiver.try_recv() {
            execute_clipboard_task(task).await;
        }
        break;
    }
}

/// 在阻塞线程池执行单个剪贴板任务。
async fn execute_clipboard_task(task: ClipboardTask) {
    if let Err(error) = tokio::task::spawn_blocking(move || task.execute()).await {
        tracing::warn!("等待系统剪贴板任务失败: {error}");
    }
}

/// 串行消费选区持久化任务；停机时只保留最后一项。
async fn run_selection_writer(
    mut receiver: mpsc::Receiver<Rect>,
    cancel: snapin_runtime::CancellationToken,
) {
    loop {
        let rect = tokio::select! {
            _ = cancel.cancelled() => None,
            rect = receiver.recv() => rect,
        };
        if let Some(rect) = rect {
            execute_selection_write(rect).await;
            continue;
        }
        // 停机时只保留最后选区，避免对中间状态做多次无意义落盘。
        let mut latest = None;
        while let Ok(rect) = receiver.try_recv() {
            latest = Some(rect);
        }
        if let Some(rect) = latest {
            execute_selection_write(rect).await;
        }
        break;
    }
}

/// 在阻塞线程池提交历史选区持久化。
async fn execute_selection_write(rect: Rect) {
    let result = tokio::task::spawn_blocking(move || {
        save_last_selection((rect.x, rect.y, rect.width, rect.height));
    })
    .await;
    if let Err(error) = result {
        tracing::warn!("等待历史选区写入任务失败: {error}");
    }
}

impl DesktopPort for AppDesktop {
    fn load_last_selection(&self) -> Option<Rect> {
        **self.last_selection.load()
    }

    fn save_last_selection(&self, rect: Rect) {
        self.last_selection.store(Arc::new(Some(rect)));
        if let Err(error) = self.selection_tx.try_send(rect) {
            tracing::warn!("提交历史选区写入任务失败: {}", queue_error_message(error));
        }
    }

    fn write_clipboard(
        &self,
        write: ClipboardWrite,
        sink: ClipboardSink,
    ) -> Result<ClipboardJobId, PortError> {
        self.submit_clipboard(ClipboardRequest::Write(write), sink)
    }

    fn read_clipboard(&self, sink: ClipboardSink) -> Result<ClipboardJobId, PortError> {
        self.submit_clipboard(ClipboardRequest::Read, sink)
    }

    fn open_in_file_manager(&self, path: PathBuf) {
        open_path_in_file_manager(path);
    }

    fn open_url(&self, url: &str) -> Result<(), PortError> {
        open_url(url).map_err(|error| {
            PortError::with_kind(
                PortErrorKind::InvalidInput,
                "打开网页链接",
                error.to_string(),
            )
        })
    }

    fn notify_copy(&self, body: &str) {
        let config = config_snapshot(&self.config);
        if !config.output.notify_on_copy {
            return;
        }
        let body = body.to_string();
        let title = "Snapin 贴图".to_string();
        if let Err(error) = self.runtime.spawn_in(TaskPool::Notify, async move {
            if let Err(error) = send_desktop_notification(&title, &body).await {
                tracing::debug!("发送桌面通知失败或系统通知不可用: {error}");
            }
        }) {
            tracing::warn!("提交桌面通知任务失败: {error}");
        }
    }
}

fn worker_start_error(operation: &str, error: snapin_runtime::TaskRuntimeError) -> PortError {
    PortError::with_kind(PortErrorKind::Unavailable, operation, error.to_string())
}

fn queue_error<T>(operation: &str, error: TrySendError<T>) -> PortError {
    let (kind, message) = match error {
        TrySendError::Full(_) => (PortErrorKind::Busy, "桌面任务队列已满"),
        TrySendError::Closed(_) => (PortErrorKind::Unavailable, "桌面任务已停止"),
    };
    PortError::with_kind(kind, operation, message)
}

fn queue_error_message(error: TrySendError<Rect>) -> String {
    queue_error("保存历史选区", error).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 任务异常退出时析构守卫必须向 UI 回传可识别错误，避免状态永久等待。
    #[test]
    fn test_clipboard_task_drop_reports_internal_failure() {
        let received = Arc::new(Mutex::new(None));
        let sink_received = received.clone();
        let task = ClipboardTask {
            id: ClipboardJobId::new(7),
            request: None,
            sink: Some(Box::new(move |_, result| {
                *sink_received
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(result);
            })),
        };
        drop(task);

        let result = received
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .expect("析构守卫必须回传失败结果");
        assert!(matches!(result, Err(PortError { .. })));
    }

    /// 队列拒绝的任务不得触发回调，避免 UI 收到从未登记过的伪完成事件。
    #[test]
    fn test_clipboard_task_disarm_suppresses_callback() {
        let count = Arc::new(AtomicU64::new(0));
        let sink_count = count.clone();
        let mut task = ClipboardTask {
            id: ClipboardJobId::new(9),
            request: None,
            sink: Some(Box::new(move |_, _| {
                sink_count.fetch_add(1, Ordering::Relaxed);
            })),
        };
        task.disarm();
        drop(task);

        assert_eq!(count.load(Ordering::Relaxed), 0);
    }

    /// 正常完成后析构不得重复调用回调。
    #[test]
    fn test_clipboard_task_completes_once() {
        let count = Arc::new(AtomicU64::new(0));
        let sink_count = count.clone();
        let mut task = ClipboardTask {
            id: ClipboardJobId::new(8),
            request: None,
            sink: Some(Box::new(move |_, _| {
                sink_count.fetch_add(1, Ordering::Relaxed);
            })),
        };
        task.complete(Ok(ClipboardResult::Written));
        drop(task);

        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
}
