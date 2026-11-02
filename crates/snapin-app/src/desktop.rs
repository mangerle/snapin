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

use crate::config::{config_snapshot, SharedConfig};
use snapin_platform::{
    open_path_in_file_manager, open_url, read_clipboard_image, read_clipboard_text,
    send_desktop_notification, write_image_to_clipboard, write_text_to_clipboard,
};
use snapin_runtime::{TaskPool, TaskRuntime};
use snapin_storage::{load_last_selection, save_last_selection};
use snapin_ui::snipper::Rect;
use snapin_ui::{DesktopPort, PortError, PortErrorKind};
use std::path::PathBuf;

/// 桌面副作用端口实现。
pub(crate) struct AppDesktop {
    config: SharedConfig,
    runtime: TaskRuntime,
}

impl AppDesktop {
    /// 构建桌面端口。
    pub(crate) fn new(config: SharedConfig, runtime: TaskRuntime) -> Self {
        Self { config, runtime }
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
        write_text_to_clipboard(text).map_err(|error| {
            PortError::with_kind(
                PortErrorKind::Unavailable,
                "写入系统剪贴板",
                error.to_string(),
            )
        })
    }

    fn write_image_clipboard(&self, image: &image::RgbaImage) -> Result<(), PortError> {
        write_image_to_clipboard(image).map_err(|error| {
            PortError::with_kind(
                PortErrorKind::Unavailable,
                "写入系统剪贴板",
                error.to_string(),
            )
        })
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
