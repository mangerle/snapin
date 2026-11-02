//! 应用层媒体任务执行器门面：统一运行时提交、取消与 OCR 引擎缓存。
//!
//! # 设计原理
//!
//! 截图与贴图窗口共享同一个进程级媒体任务执行器，任务统一提交到
//! [`snapin_runtime::TaskRuntime`]，不再自建工作线程与同步队列。任务准入、
//! 并发上限、panic 观测与退出等待由运行时统一负责；本模块只保留媒体领域状态：
//! - 提交时固化配置快照，配置热更新只影响后续任务；
//! - 每个任务持有独立取消标志，取消后丢弃结果而不是污染 UI；
//! - OCR 引擎按模型目录缓存，避免每个任务重复加载数十兆模型。
//!
//! 具体 OCR/翻译/保存实现拆分到子模块：
//! - `worker`：任务分派与 OCR 引擎缓存；
//! - `save`：截图/贴图保存流程；
//! - `translate`：翻译图元排版与服务商映射。
//!
//! # 代价与权衡
//!
//! 媒体任务与通知任务共享应用运行时的并发名额；如果未来需要让 OCR 独占一组
//! worker，应为媒体任务单独创建一个 [`snapin_runtime::TaskRuntime`]，
//! 而不是重新引入独立线程池。

mod save;
mod translate;
mod worker;

use crate::config::{config_snapshot, SharedConfig};
use snapin_runtime::TaskPool;
use snapin_service::ocr::LocalPpOcrEngine;
use snapin_ui::{
    JobCancel, JobId, MediaJob, MediaJobOutcome, MediaJobRunner, PortError, PortErrorKind,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// OCR 引擎缓存；键为模型目录，初始化失败也按目录缓存，避免重复扫描。
#[derive(Default)]
pub(super) struct OcrCache {
    pub(super) entry: Option<(String, Result<Arc<LocalPpOcrEngine>, String>)>,
}

/// 进程级媒体任务执行器。
pub(crate) struct MediaService {
    config: SharedConfig,
    runtime: snapin_runtime::TaskRuntime,
    ocr_cache: Arc<Mutex<OcrCache>>,
    next_job_id: AtomicU64,
}

impl MediaService {
    /// 使用共享配置与统一后台运行时构建执行器。
    pub(crate) fn new(config: SharedConfig, runtime: snapin_runtime::TaskRuntime) -> Self {
        Self {
            config,
            runtime,
            ocr_cache: Arc::new(Mutex::new(OcrCache::default())),
            next_job_id: AtomicU64::new(1),
        }
    }
}

impl MediaJobRunner for MediaService {
    fn submit(
        &self,
        job: MediaJob,
        sink: Box<dyn FnOnce(JobId, MediaJobOutcome) + Send>,
    ) -> Result<(JobId, JobCancel), PortError> {
        let job_id = JobId::new(self.next_job_id.fetch_add(1, Ordering::Relaxed));
        let config = config_snapshot(&self.config);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = cancel.clone();
        let task_cancel = cancel.clone();
        let worker_cancel = task_cancel.clone();
        let cache = self.ocr_cache.clone();

        self.runtime
            .spawn_in(TaskPool::Media, async move {
                let outcome = worker::execute_job(job, config, cache, worker_cancel).await;
                if !task_cancel.load(Ordering::Acquire) {
                    sink(job_id, outcome);
                }
            })
            .map_err(|error| {
                PortError::with_kind(PortErrorKind::Busy, "提交媒体任务", error.to_string())
            })?;

        Ok((
            job_id,
            Box::new(move || cancel_flag.store(true, Ordering::Release)),
        ))
    }
}
