//! Snapin 统一后台任务运行时。
//!
//! # 设计原理
//!
//! 工程内曾同时存在「Tokio 任务」「专用工作线程」「每次点击派生线程」「子进程
//! reaper 线程」四套后台执行方式，取消、停机、panic 观测与并发上限各自为政。
//! 本 crate 把“异步任务 + 阻塞任务”的提交、准入与追踪收敛到同一份契约：
//!
//! - 所有任务挂在同一个 [`TaskTracker`] 上，退出时可以等待全部完成；
//! - 用固定容量的 [`Semaphore`] 做准入控制，队列满时快速失败而不是无界积压；
//! - 通过 [`CancellationToken`] 发布取消信号，任务自行决定检查点；
//! - 若调用方已有 Tokio 运行时则复用，否则由 [`TaskRuntime::shared`] 持有唯一的
//!   进程级运行时，避免每个功能各建一座运行时。
//!
//! # 代价与权衡
//!
//! 阻塞任务（OCR 推理、图像编码）仍必须使用 `spawn_blocking`，否则会占满异步
//! worker；本 crate 提供 [`TaskRuntime::spawn_blocking`] 的统一包装，但没有能力
//! 强制中断已经进入阻塞线程的任务。因此取消语义是“丢弃结果 + 尽快返回”，
//! 不是内核级抢占。

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use thiserror::Error;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
pub use tokio_util::sync::CancellationToken;
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
}

impl TaskPool {
    const fn index(self) -> usize {
        match self {
            Self::Default => 0,
            Self::Media => 1,
            Self::Notify => 2,
            Self::Process => 3,
            Self::Maintenance => 4,
        }
    }
}

/// 单个任务池的并发名额。
struct Pool {
    permits: Arc<Semaphore>,
    capacity: usize,
}

impl Pool {
    fn new(capacity: usize) -> Self {
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
    id: u64,
    abort: tokio::task::AbortHandle,
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
struct RuntimeInner {
    handle: Handle,
    tracker: TaskTracker,
    pools: [Pool; 5],
    cancel: CancellationToken,
    shutting_down: AtomicBool,
    next_task_id: AtomicU64,
    /// 仅进程级共享运行时持有；普通调用方复用当前运行时。
    _owned: Option<Runtime>,
}

/// 统一后台任务运行时句柄。
///
/// 克隆代价为一次 `Arc` 递增，可安全跨线程共享。提交任务不会阻塞调用线程；
/// 准入失败立即返回 [`TaskRuntimeError::QueueFull`]。
#[derive(Clone)]
pub struct TaskRuntime {
    inner: Arc<RuntimeInner>,
}

impl TaskRuntime {
    /// 复用当前线程的 Tokio 运行时，并设置任务并发上限。
    ///
    /// # Errors
    ///
    /// 当前线程没有 Tokio 运行时上下文时返回 [`TaskRuntimeError::NoRuntime`]。
    pub fn current(capacity: usize) -> Result<Self, TaskRuntimeError> {
        let handle = Handle::try_current().map_err(|_| TaskRuntimeError::NoRuntime)?;
        Ok(Self::from_handle_inner(handle, capacity, None))
    }

    /// 获取进程级共享任务运行时；首次调用时创建并常驻。
    ///
    /// 共享实例使用固定的分池容量（默认 16、媒体 4、通知 4、进程 8、维护 2），
    /// 避免不同模块按各自参数竞争首次初始化而得到不可预测的契约。用于没有
    /// Tokio 上下文的平台适配代码（例如同步的系统命令派生）。创建失败时返回
    /// `None`，调用方应降级为同步执行或记录警告。
    pub fn shared() -> Option<&'static Self> {
        static SHARED: OnceLock<Option<TaskRuntime>> = OnceLock::new();
        SHARED
            .get_or_init(|| {
                let runtime = Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("snapin-shared")
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => {
                        let handle = runtime.handle().clone();
                        Some(Self::from_handle_inner(handle, 16, Some(runtime)))
                    }
                    Err(error) => {
                        tracing::error!("创建进程级后台任务运行时失败: {error}");
                        None
                    }
                }
            })
            .as_ref()
    }

    /// 由已有 Tokio 句柄构造任务运行时。
    ///
    /// 守护进程需要把 GUI 事件循环放在主线程，同时把异步服务放进独立运行时；
    /// 此时由调用方持有所需运行时，并通过本构造函数复用其句柄。
    pub fn from_handle(handle: Handle, capacity: usize) -> Self {
        Self::from_handle_inner(handle, capacity, None)
    }

    /// 由已有句柄构造运行时；`owned` 仅用于进程级共享实例保活。
    fn from_handle_inner(handle: Handle, capacity: usize, owned: Option<Runtime>) -> Self {
        let capacity = capacity.max(1);
        let media_capacity = (capacity / 2).max(2);
        let pools = [
            Pool::new(capacity),
            Pool::new(media_capacity),
            Pool::new(4),
            Pool::new(8),
            Pool::new(2),
        ];
        Self {
            inner: Arc::new(RuntimeInner {
                handle,
                tracker: TaskTracker::new(),
                pools,
                cancel: CancellationToken::new(),
                shutting_down: AtomicBool::new(false),
                next_task_id: AtomicU64::new(1),
                _owned: owned,
            }),
        }
    }

    /// 向默认池提交一个异步任务。
    ///
    /// # Errors
    ///
    /// 并发任务达到上限时返回 [`TaskRuntimeError::QueueFull`]，运行时停机后返回
    /// [`TaskRuntimeError::ShuttingDown`]。
    pub fn spawn<F>(&self, task: F) -> Result<TaskHandle, TaskRuntimeError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_in(TaskPool::Default, task)
    }

    /// 向指定任务池提交异步任务。
    ///
    /// # Errors
    ///
    /// 运行时已停机时返回 [`TaskRuntimeError::ShuttingDown`]；目标池名额耗尽时返回
    /// [`TaskRuntimeError::QueueFull`]。
    pub fn spawn_in<F>(&self, pool: TaskPool, task: F) -> Result<TaskHandle, TaskRuntimeError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if self.inner.shutting_down.load(Ordering::Acquire) {
            return Err(TaskRuntimeError::ShuttingDown);
        }
        let permit = self.acquire(pool)?;
        let id = self.inner.next_task_id.fetch_add(1, Ordering::Relaxed);
        let join = self.inner.handle.spawn(async move {
            let _permit = permit;
            task.await;
        });
        let abort = join.abort_handle();
        // 监视 JoinHandle 以感知任务 panic；监控 Future 本身也纳入 TaskTracker。
        let tracked = self.inner.tracker.track_future(async move {
            if let Err(error) = join.await {
                if error.is_panic() {
                    tracing::error!("后台异步任务发生 panic，任务序号: {id}，已隔离该任务");
                } else {
                    tracing::debug!("后台异步任务在完成前被取消，任务序号: {id}");
                }
            }
        });
        drop(self.inner.handle.spawn(tracked));
        Ok(TaskHandle { id, abort })
    }

    /// 向默认池提交一个阻塞任务到 Tokio 阻塞线程池。
    ///
    /// # Errors
    ///
    /// 并发任务达到上限时返回 [`TaskRuntimeError::QueueFull`]，运行时停机后返回
    /// [`TaskRuntimeError::ShuttingDown`]。
    pub fn spawn_blocking<F, R>(&self, task: F) -> Result<TaskHandle, TaskRuntimeError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.spawn_blocking_in(TaskPool::Default, task)
    }

    /// 向指定任务池提交阻塞任务。
    ///
    /// # Errors
    ///
    /// 运行时已停机时返回 [`TaskRuntimeError::ShuttingDown`]；目标池名额耗尽时返回
    /// [`TaskRuntimeError::QueueFull`]。
    pub fn spawn_blocking_in<F, R>(
        &self,
        pool: TaskPool,
        task: F,
    ) -> Result<TaskHandle, TaskRuntimeError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        if self.inner.shutting_down.load(Ordering::Acquire) {
            return Err(TaskRuntimeError::ShuttingDown);
        }
        let permit = self.acquire(pool)?;
        let id = self.inner.next_task_id.fetch_add(1, Ordering::Relaxed);
        let join = self.inner.handle.spawn_blocking(task);
        let abort = join.abort_handle();
        let tracked = self.inner.tracker.track_future(async move {
            let _permit = permit;
            match join.await {
                Ok(_) => {}
                Err(error) if error.is_panic() => {
                    tracing::error!("后台阻塞任务发生 panic，任务序号: {id}，已隔离该任务");
                }
                Err(_) => tracing::debug!("后台阻塞任务在完成前被取消，任务序号: {id}"),
            }
        });
        drop(self.inner.handle.spawn(tracked));
        Ok(TaskHandle { id, abort })
    }

    /// 获取共享取消令牌。
    ///
    /// 任务应在耗时循环或网络等待处用 `select!` 监听该令牌，
    /// 或在关键检查点调用 `is_cancelled()`。
    pub fn cancellation_token(&self) -> CancellationToken {
        self.inner.cancel.clone()
    }

    /// 当前允许的最大并发任务数。
    pub fn capacity(&self) -> usize {
        self.inner.pools[TaskPool::Default.index()].capacity
    }

    /// 请求取消并等待已提交任务结束，最多等待 `timeout`。
    ///
    /// 返回 `true` 表示在时限内排空全部任务；返回 `false` 表示仍有任务未退出
    /// （例如正在执行的阻塞式 ONNX 推理无法被强制中断），调用方可以继续执行进程
    /// 退出流程，但必须记录仍在运行的任务，避免无限等待。
    ///
    /// 该方法只能在 Tokio 运行时之外或独立运行时中调用；在异步任务内部调用
    /// 会等待自身而永久挂起，调用方必须避免自等待。
    pub async fn shutdown(&self, timeout: Duration) -> bool {
        self.inner.shutting_down.store(true, Ordering::Release);
        self.inner.cancel.cancel();
        self.inner.tracker.close();

        if timeout.is_zero() {
            self.inner.tracker.wait().await;
            return true;
        }

        match tokio::time::timeout(timeout, self.inner.tracker.wait()).await {
            Ok(()) => true,
            Err(_) => {
                tracing::warn!(
                    "等待后台任务退出超时，已等待 {}ms；仍有任务未响应取消，继续执行退出流程",
                    timeout.as_millis()
                );
                false
            }
        }
    }

    /// 在非异步线程上同步等待停机完成，最多等待 `timeout`。
    ///
    /// 供事件循环退出、进程收尾等同步上下文使用；若在 Tokio 运行时内部调用
    /// `Handle::block_on` 会 panic，因此异步任务中必须使用 [`Self::shutdown`]。
    pub fn shutdown_blocking(&self, timeout: Duration) -> bool {
        self.inner.handle.block_on(self.shutdown(timeout))
    }

    /// 尝试获取指定任务池的并发名额；失败时给出该池的明确容量。
    fn acquire(&self, pool: TaskPool) -> Result<OwnedSemaphorePermit, TaskRuntimeError> {
        let pool = &self.inner.pools[pool.index()];
        pool.permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| TaskRuntimeError::QueueFull {
                capacity: pool.capacity,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// 提交的任务应被追踪，关闭后等待其完成。
    #[tokio::test]
    async fn test_tasks_are_tracked_and_waited() {
        let runtime = TaskRuntime::current(4).expect("测试运行时必须可用");
        let completed = Arc::new(AtomicUsize::new(0));
        for _ in 0..3 {
            let completed = completed.clone();
            runtime
                .spawn(async move {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    completed.fetch_add(1, Ordering::AcqRel);
                })
                .expect("任务应成功入队");
        }
        assert!(runtime.shutdown(Duration::from_secs(1)).await);
        assert_eq!(completed.load(Ordering::Acquire), 3);
    }

    /// 并发名额满时必须快速失败，而不是无界排队。
    #[tokio::test]
    async fn test_queue_capacity_is_enforced() {
        let runtime = TaskRuntime::current(1).expect("测试运行时必须可用");
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        runtime
            .spawn(async move {
                let _ = release_rx.await;
            })
            .expect("首个任务应成功入队");

        let second = runtime.spawn(async {});
        assert!(matches!(
            second,
            Err(TaskRuntimeError::QueueFull { capacity: 1 })
        ));
        let _ = release_tx.send(());
        assert!(runtime.shutdown(Duration::from_secs(1)).await);
    }

    /// 阻塞任务也必须纳入统一追踪。
    #[tokio::test]
    async fn test_blocking_tasks_are_tracked() {
        let runtime = TaskRuntime::current(2).expect("测试运行时必须可用");
        let flag = Arc::new(AtomicUsize::new(0));
        let task_flag = flag.clone();
        runtime
            .spawn_blocking(move || task_flag.fetch_add(1, Ordering::AcqRel))
            .expect("阻塞任务应成功入队");
        assert!(runtime.shutdown(Duration::from_secs(1)).await);
        assert_eq!(flag.load(Ordering::Acquire), 1);
    }

    /// 不响应取消的任务不得让停机等待无限挂起，超时应返回 false 供调用方兜底退出。
    #[tokio::test]
    async fn test_shutdown_times_out_when_task_ignores_cancellation() {
        let runtime = TaskRuntime::current(1).expect("测试运行时必须可用");
        runtime
            .spawn(async {
                // 模拟无法被内核级抢占的阻塞式推理：不检查取消令牌，持续挂起。
                tokio::time::sleep(Duration::from_secs(30)).await;
            })
            .expect("任务应成功入队");

        let drained = runtime.shutdown(Duration::from_millis(10)).await;
        assert!(!drained, "超时后必须如实返回未排空");
    }

    /// 停机完成后必须拒绝新任务，不能返回 Ok 让调用方误以为任务已被纳管。
    #[tokio::test]
    async fn test_spawn_after_shutdown_is_rejected() {
        let runtime = TaskRuntime::current(2).expect("测试运行时必须可用");
        assert!(runtime.shutdown(Duration::from_secs(1)).await);
        assert!(matches!(
            runtime.spawn(async {}),
            Err(TaskRuntimeError::ShuttingDown)
        ));
    }

    /// 长驻维护池与媒体池不可互相挤占名额。
    #[tokio::test]
    async fn test_pools_have_independent_capacity() {
        let runtime = TaskRuntime::current(1).expect("测试运行时必须可用");
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        runtime
            .spawn(async move {
                let _ = release_rx.await;
            })
            .expect("默认池首个任务应成功入队");

        // 默认池已满，但媒体池仍应保有自己的名额。
        assert!(matches!(
            runtime.spawn(async {}),
            Err(TaskRuntimeError::QueueFull { capacity: 1 })
        ));
        runtime
            .spawn_in(TaskPool::Media, async {})
            .expect("媒体池不应被默认池占满");

        let _ = release_tx.send(());
        assert!(runtime.shutdown(Duration::from_secs(1)).await);
    }
}
