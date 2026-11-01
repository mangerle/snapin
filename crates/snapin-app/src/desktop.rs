//! 桌面副作用端口实现：选区缓存、剪贴板、文件管理器与桌面通知。
//!
//! # 设计原理
//!
//! 表现层只通过 [`DesktopPort`] 发起用户动作，本模块把调用转交给
//! `snapin-platform`，并把平台错误转换为统一的 [`PortError`]。桌面通知由
//! 独立有界通道与工作线程发送，避免在 winit 事件循环中阻塞等待 D-Bus。
//!
//! # 代价与权衡
//!
//! 通知采用“允许丢弃”的快速失败策略：队列满时丢弃最新一条并记录警告，
//! 因为通知属于非关键提示，不能为了可靠投递反过来阻塞用户交互。

use crate::config::{config_snapshot, SharedConfig};
use crate::error::AppError;
use snapin_platform::{
    load_last_selection, open_path_in_file_manager, read_clipboard_image, read_clipboard_text,
    save_last_selection, send_desktop_notification, write_image_to_clipboard,
    write_text_to_clipboard,
};
use snapin_ui::snipper::Rect;
use snapin_ui::{DesktopPort, PortError};
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

/// 通知队列容量：通知是低频提示，允许少量积压，但绝不允许无界增长。
const NOTIFICATION_QUEUE_CAPACITY: usize = 16;

/// 单条待发送的桌面通知。
struct Notification {
    title: String,
    body: String,
}

/// 桌面通知异步发送服务。
///
/// 工作线程持有独立的 current-thread Tokio 运行时，按顺序调用 D-Bus 通知接口；
/// 析构时关闭通道并等待线程退出，保证不遗留脱缰线程。
struct NotificationService {
    sender: Option<SyncSender<Notification>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl NotificationService {
    fn new() -> Result<Self, AppError> {
        let (sender, receiver) = sync_channel(NOTIFICATION_QUEUE_CAPACITY);
        let worker = thread::Builder::new()
            .name("snapin-notification".to_string())
            .spawn(move || notification_loop(receiver))
            .map_err(|error| AppError::NotificationSpawn {
                reason: error.to_string(),
            })?;
        Ok(Self {
            sender: Some(sender),
            worker: Mutex::new(Some(worker)),
        })
    }

    fn notify(&self, title: String, body: String) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        match sender.try_send(Notification { title, body }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::warn!("桌面通知队列已满，已丢弃最新通知");
            }
            Err(TrySendError::Disconnected(_)) => {
                tracing::debug!("桌面通知工作线程已退出，通知未发送");
            }
        }
    }
}

impl Drop for NotificationService {
    fn drop(&mut self) {
        self.sender = None;
        let worker = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(worker) = worker {
            if worker.join().is_err() {
                tracing::warn!("桌面通知工作线程退出时发生 panic");
            }
        }
    }
}

/// 通知工作线程主循环。
fn notification_loop(receiver: Receiver<Notification>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        tracing::warn!("创建桌面通知运行时失败，桌面通知不可用");
        return;
    };

    while let Ok(notification) = receiver.recv() {
        if let Err(error) = runtime.block_on(send_desktop_notification(
            &notification.title,
            &notification.body,
        )) {
            tracing::debug!("发送桌面通知失败或系统通知不可用: {error}");
        }
    }
}

/// 桌面副作用端口实现。
pub(crate) struct AppDesktop {
    config: SharedConfig,
    notifications: NotificationService,
}

impl AppDesktop {
    /// 构建桌面端口并启动通知工作线程。
    ///
    /// # Errors
    ///
    /// 通知工作线程创建失败时返回 [`AppError::NotificationSpawn`]。
    pub(crate) fn new(config: SharedConfig) -> Result<Self, AppError> {
        Ok(Self {
            config,
            notifications: NotificationService::new()?,
        })
    }
}

impl DesktopPort for AppDesktop {
    fn load_last_selection(&self) -> Option<Rect> {
        load_last_selection().map(|(x, y, width, height)| Rect {
            x,
            y,
            width,
            height,
        })
    }

    fn save_last_selection(&self, rect: Rect) {
        save_last_selection((rect.x, rect.y, rect.width, rect.height));
    }

    fn write_text_clipboard(&self, text: &str) -> Result<(), PortError> {
        write_text_to_clipboard(text)
            .map_err(|error| PortError::new("写入系统剪贴板", error.to_string()))
    }

    fn write_image_clipboard(&self, image: &image::RgbaImage) -> Result<(), PortError> {
        write_image_to_clipboard(image)
            .map_err(|error| PortError::new("写入系统剪贴板", error.to_string()))
    }

    fn read_clipboard_image(&self) -> Option<image::RgbaImage> {
        match read_clipboard_image() {
            Ok(image) => image,
            Err(error) => {
                tracing::debug!("读取剪贴板图像失败: {error}");
                None
            }
        }
    }

    fn read_clipboard_text(&self) -> Option<String> {
        match read_clipboard_text() {
            Ok(text) => text,
            Err(error) => {
                tracing::debug!("读取剪贴板文本失败: {error}");
                None
            }
        }
    }

    fn open_in_file_manager(&self, path: PathBuf) {
        open_path_in_file_manager(path);
    }

    fn notify_copy(&self, body: &str) {
        let config = config_snapshot(&self.config);
        if !config.output.notify_on_copy {
            return;
        }
        self.notifications
            .notify("Snapin 贴图".to_string(), body.to_string());
    }
}
