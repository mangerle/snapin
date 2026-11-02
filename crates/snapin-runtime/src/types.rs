//! 任务运行时公共类型与内部状态。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::runtime::{Handle, Runtime};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// 后台任务运行时错误。
#[derive(Debug, Error)]
pub enum TaskRuntimeError {
    /// 当前线程没有可复用的 Tokio 运行时
    #[error("当前线程没有可用的 Tokio 运行时，无法提交后台任务")]
    NoRuntime,

    /// 任务准入队列已满
    #[error("后台任务队列已满，当前并发上限为 {capacity}")]
    QueueFull {
        /// 当前运行时允许的最大并发任务数
        capacity: usize,
    },

    /// 进程级共享运行时构建失败
    #[error("创建进程级后台 Tokio 运行时失败，原因: {reason}")]
    RuntimeBuild {
        /// 底层运行时构建错误描述
        reason: String,
    },

    /// 运行时已进入停机流程，不再接受新任务
    #[error("后台任务运行时已停止，拒绝提交新任务")]
    ShuttingDown,

    /// 阻塞任务在完成前 panic 或 JoinHandle 被运行时取消
    #[error("后台阻塞任务执行失败，任务序号: {task_id}，原因: {reason}")]
    BlockingTaskFailed {
        /// 任务序号
        task_id: u64,
        /// 中文失败原因
        reason: String,
    },
}

/// 任务池划分。
///
/// 长驻维护任务、媒体推理、桌面通知与系统子进程的并发特征完全不同，必须使用
/// 独立名额，避免一个 60 秒延时截图或常驻更新巡检占满全部容量后让热键静默失效。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPool {
    /// 通用短任务与 IPC 命令
    Default,
    /// OCR、翻译、图像编码等媒体任务
    Media,
    /// 桌面通知等非关键副作用
    Notify,
    /// 外部系统子进程
    Process,
    /// 常驻维护循环（更新巡检等）
    Maintenance,
    /// 剪贴板轮询、缓存索引与状态文件等独立阻塞 I/O
    Io,
}

impl TaskPool {
    pub(super) const fn index(self) -> usize {
        match self {
            Self::Default => 0,
            Self::Media => 1,
            Self::Notify => 2,
            Self::Process => 3,
            Self::Maintenance => 4,
            Self::Io => 5,
        }
    }
}

/// 单个任务池的并发名额。
pub(super) struct Pool {
    pub(super) permits: Arc<Semaphore>,
    pub(super) capacity: usize,
}

impl Pool {
    pub(super) fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            permits: Arc::new(Semaphore::new(capacity)),
            capacity,
        }
    }
}

/// 后台任务句柄。
///
/// 调用方需要中止任务或观测任务标识时使用；丢弃句柄不会取消任务，
/// 任务仍由运行时纳管并在停机时等待。
#[derive(Debug, Clone)]
pub struct TaskHandle {
    pub(super) id: u64,
    pub(super) abort: tokio::task::AbortHandle,
}

impl TaskHandle {
    /// 任务唯一序号。
    pub fn id(&self) -> u64 {
        self.id
    }

    /// 请求中止任务；阻塞任务一旦开始执行可能无法被真正中断。
    pub fn abort(&self) {
        self.abort.abort();
    }
}

/// 任务运行时内部状态。
pub(super) struct RuntimeInner {
    pub(super) handle: Handle,
    pub(super) tracker: TaskTracker,
    pub(super) pools: [Pool; 6],
    pub(super) cancel: CancellationToken,
    pub(super) shutting_down: AtomicBool,
    /// 串行化任务登记与停机切换，消除“检查未停机后又把任务登记到已关闭 Tracker”的窗口。
    pub(super) admission: Mutex<()>,
    pub(super) next_task_id: AtomicU64,
    /// 仍在运行的任务中止句柄；停机超时后统一 abort，避免异步任务继续存活。
    pub(super) active_tasks: Mutex<HashMap<u64, tokio::task::AbortHandle>>,
    /// 仅进程级共享运行时持有；普通调用方复用当前运行时。
    pub(super) _owned: Option<Runtime>,
}

/// 统一后台任务运行时句柄。
///
/// 克隆代价为一次 `Arc` 递增，可安全跨线程共享。提交任务不会阻塞调用线程；
/// 准入失败立即返回 [`TaskRuntimeError::QueueFull`]。
#[derive(Clone)]
pub struct TaskRuntime {
    pub(super) inner: Arc<RuntimeInner>,
}
