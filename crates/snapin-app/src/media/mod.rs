//! 应用层媒体任务执行器门面：有界队列、取消与工作线程生命周期。
//!
//! # 设计原理
//!
//! 截图与贴图窗口共享同一个进程级媒体执行器，避免两侧各加载一次 ONNX 模型、
//! 各建一套翻译器缓存。执行器使用固定容量同步通道形成背压，工作线程内持有一个
//! current-thread Tokio 运行时承载翻译 HTTP 请求与 Portal 保存对话框。
//!
//! 每个任务在提交时固化配置快照与取消标志：配置热更新只影响后续任务，
//! 已完成任务不会因配置切换而出现半新半旧的行为；取消只丢弃结果，不阻塞 UI。
//!
//! 具体 OCR/翻译/保存实现拆分到子模块，避免单文件持续膨胀：
//! - `worker`：工作线程与 OCR/翻译缓存；
//! - `save`：截图/贴图保存流程；
//! - `translate`：翻译图元排版与服务商映射。
//!
//! # 代价与权衡
//!
//! 采用单工作线程串行执行，换来实现简单与 OCR 模型只加载一次；如果未来需要
//! 并行处理多张大图，应改为多工作线程加信号量，而不是在 UI 侧再起一套执行器。

mod save;
mod translate;
mod worker;

use crate::config::{config_snapshot, SharedConfig};
use crate::error::AppError;
use snapin_ui::{JobCancel, MediaJob, MediaJobOutcome, MediaJobRunner, PortError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// 媒体任务队列容量：工作线程执行一个、排队一个，更多任务快速失败形成背压。
const JOB_QUEUE_CAPACITY: usize = 2;

/// 工作线程消息。
pub(super) enum WorkerMessage {
    /// 执行一个媒体任务
    Job(WorkItem),
    /// 请求工作线程退出
    Stop,
}

/// 一个待执行的媒体任务及其结果回调。
pub(super) struct WorkItem {
    pub(super) job: MediaJob,
    pub(super) config: Arc<snapin_core::config::Config>,
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) sink: Box<dyn FnOnce(MediaJobOutcome) + Send>,
}

/// 媒体执行器内部状态。
struct MediaServiceInner {
    sender: Mutex<Option<SyncSender<WorkerMessage>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    stopping: Arc<AtomicBool>,
}

impl MediaServiceInner {
    /// 请求停机并等待工作线程退出。
    fn shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(sender) = sender {
            let _ = sender.try_send(WorkerMessage::Stop);
        }

        let worker = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(worker) = worker {
            if worker.join().is_err() {
                tracing::error!("媒体任务工作线程退出时发生 panic");
            }
        }
    }
}

impl Drop for MediaServiceInner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 进程级媒体任务执行器。
#[derive(Clone)]
pub(crate) struct MediaService {
    inner: Arc<MediaServiceInner>,
    config: SharedConfig,
}

impl MediaService {
    /// 创建执行器并拉起命名工作线程。
    ///
    /// # Errors
    ///
    /// 工作线程创建失败时返回 [`AppError::WorkerSpawn`]。
    pub(crate) fn new(config: SharedConfig) -> Result<Self, AppError> {
        let (sender, receiver) = sync_channel(JOB_QUEUE_CAPACITY);
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stopping = stopping.clone();
        let worker = thread::Builder::new()
            .name("snapin-media-job".to_string())
            .spawn(move || worker::worker_loop(receiver, worker_stopping))
            .map_err(|error| AppError::WorkerSpawn {
                reason: error.to_string(),
            })?;

        Ok(Self {
            inner: Arc::new(MediaServiceInner {
                sender: Mutex::new(Some(sender)),
                worker: Mutex::new(Some(worker)),
                stopping,
            }),
            config,
        })
    }
}

impl MediaJobRunner for MediaService {
    fn submit(
        &self,
        job: MediaJob,
        sink: Box<dyn FnOnce(MediaJobOutcome) + Send>,
    ) -> Result<JobCancel, PortError> {
        let config = config_snapshot(&self.config);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = cancel.clone();
        let item = WorkItem {
            job,
            config,
            cancel,
            sink,
        };

        let sender = self
            .inner
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(sender) = sender.as_ref() else {
            return Err(PortError::new("提交媒体任务", "媒体执行器已停止"));
        };
        match sender.try_send(WorkerMessage::Job(item)) {
            Ok(()) => Ok(Box::new(move || cancel_flag.store(true, Ordering::Release))),
            Err(TrySendError::Full(_)) => Err(PortError::new(
                "提交媒体任务",
                "后台媒体队列已满，请等待当前任务完成",
            )),
            Err(TrySendError::Disconnected(_)) => {
                Err(PortError::new("提交媒体任务", "媒体工作线程已退出"))
            }
        }
    }
}
