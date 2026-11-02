//! 桌面副作用端口实现：选区缓存、剪贴板、文件管理器与桌面通知。
//!
//! # 设计原理
//!
//! 表现层只通过 [`DesktopPort`] 发起用户动作，本模块把调用转交给
//! `snapin-platform`，并把平台错误转换为统一的 [`PortError`]。桌面通知
//! 统一提交到应用级 [`TaskRuntime`]，避免在 winit 事件循环中阻塞等待 D-Bus，
//! 也不再为每条通知创建线程。
//!
//! # 代价与权衡
//!
//! 通知任务与媒体任务共享同一运行时的并发名额；通知属于非关键提示，提交失败时
//! 只记录警告，不会反向阻塞用户交互。

use crate::clipboard_cache::{ClipboardCache, ClipboardContent};
use crate::config::{config_snapshot, SharedConfig};
use arc_swap::ArcSwap;
use snapin_platform::{
    open_path_in_file_manager, open_url, send_desktop_notification, write_image_to_clipboard,
    write_text_to_clipboard,
};
use snapin_runtime::{TaskPool, TaskRuntime};
use snapin_storage::ClipboardEntryKind;
use snapin_storage::{load_last_selection, save_last_selection};
use snapin_ui::snipper::Rect;
use snapin_ui::{DesktopPort, PortError, PortErrorKind};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::{self, error::TrySendError};

/// 桌面写操作队列容量；按用户交互频率设置上限，满队列时快速失败。
const DESKTOP_WRITE_QUEUE_CAPACITY: usize = 8;

enum ClipboardWrite {
    Text(String),
    Image(Arc<image::RgbaImage>),
}

impl ClipboardWrite {
    fn execute(self) {
        match self {
            Self::Text(text) => {
                if let Err(error) = write_text_to_clipboard(&text) {
                    tracing::warn!("写入系统剪贴板失败: {error}");
                }
            }
            Self::Image(image) => {
                if let Err(error) = write_image_to_clipboard(&image) {
                    tracing::warn!("写入剪贴板图像失败: {error}");
                }
            }
        }
    }
}

/// 桌面副作用端口实现。
pub(crate) struct AppDesktop {
    config: SharedConfig,
    runtime: TaskRuntime,
    last_selection: ArcSwap<Option<Rect>>,
    clipboard_cache: Arc<ClipboardCache>,
    clipboard_tx: mpsc::Sender<ClipboardWrite>,
    selection_tx: mpsc::Sender<Rect>,
}

impl AppDesktop {
    /// 构建桌面端口。
    pub(crate) fn new(
        config: SharedConfig,
        runtime: TaskRuntime,
        clipboard_cache: Arc<ClipboardCache>,
    ) -> Result<Self, PortError> {
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
                run_clipboard_writer(clipboard_rx, cancel.clone()),
            )
            .map_err(|error| worker_start_error("启动系统剪贴板写入任务", error))?;
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
            clipboard_cache,
            clipboard_tx,
            selection_tx,
        })
    }
}

async fn run_clipboard_writer(
    mut receiver: mpsc::Receiver<ClipboardWrite>,
    cancel: snapin_runtime::CancellationToken,
) {
    loop {
        let write = tokio::select! {
            _ = cancel.cancelled() => None,
            write = receiver.recv() => write,
        };
        if let Some(write) = write {
            execute_clipboard_write(write).await;
            continue;
        }
        while let Ok(write) = receiver.try_recv() {
            execute_clipboard_write(write).await;
        }
        break;
    }
}

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

async fn execute_clipboard_write(write: ClipboardWrite) {
    if let Err(error) = tokio::task::spawn_blocking(move || write.execute()).await {
        tracing::warn!("等待系统剪贴板写入任务失败: {error}");
    }
}

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

    fn write_text_clipboard(&self, text: &str) -> Result<(), PortError> {
        let text = text.to_string();
        let content = ClipboardContent::Text {
            text: text.clone(),
            kind: ClipboardEntryKind::Text,
        };
        self.clipboard_tx
            .try_send(ClipboardWrite::Text(text))
            .map_err(|error| queue_error("提交系统剪贴板写入任务", error))?;
        self.clipboard_cache.store(content);
        Ok(())
    }

    fn write_image_clipboard(&self, image: image::RgbaImage) -> Result<(), PortError> {
        let image = Arc::new(image);
        self.clipboard_tx
            .try_send(ClipboardWrite::Image(image.clone()))
            .map_err(|error| queue_error("提交系统剪贴板写入任务", error))?;
        self.clipboard_cache.store(ClipboardContent::Image(image));
        Ok(())
    }

    fn read_clipboard_image(&self) -> Option<Arc<image::RgbaImage>> {
        self.clipboard_cache.image()
    }

    fn read_clipboard_text(&self) -> Option<String> {
        self.clipboard_cache.text()
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
        TrySendError::Full(_) => (PortErrorKind::Busy, "桌面写入队列已满"),
        TrySendError::Closed(_) => (PortErrorKind::Unavailable, "桌面写入任务已停止"),
    };
    PortError::with_kind(kind, operation, message)
}

fn queue_error_message(error: TrySendError<Rect>) -> String {
    queue_error("保存历史选区", error).to_string()
}
