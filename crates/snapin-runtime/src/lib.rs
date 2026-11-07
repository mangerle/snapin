//! Snapin 统一后台任务运行时。
//!
//! # 设计原理
//!
//! 工程内曾同时存在「Tokio 任务」「专用工作线程」「每次点击派生线程」「子进程
//! reaper 线程」四套后台执行方式，取消、停机、panic 观测与并发上限各自为政。
//! 本 crate 把“异步任务 + 阻塞任务”的提交、准入与追踪收敛到同一份契约：
//!
//! - 所有任务挂在同一个 [`TaskTracker`] 上，退出时可以等待全部完成；
//! - 用固定容量的 [`tokio::sync::Semaphore`] 做准入控制，队列满时快速失败而不是无界积压；
//! - 通过 [`CancellationToken`] 发布取消信号，任务自行决定检查点；
//! - 调用方已有 Tokio 上下文时用 [`TaskRuntime::current`] 复用；没有上下文时可用
//!   [`TaskRuntime::shared`] 获取唯一的进程级运行时，避免每个功能各建一座运行时。
//!
//! # 代价与权衡
//!
//! 阻塞任务（OCR 推理、图像编码）仍必须使用 `spawn_blocking`，否则会占满异步
//! worker；本 crate 提供 [`TaskRuntime::spawn_blocking`] 的统一包装，但没有能力
//! 强制中断已经进入阻塞线程的任务。因此取消语义是“丢弃结果 + 尽快返回”，
//! 不是内核级抢占。

mod types;

pub use types::*;

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::sync::OwnedSemaphorePermit;
pub use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

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
    /// 共享实例使用固定的分池容量（默认 16、媒体 8、通知 4、进程 8、维护 2、
    /// 独立 I/O 4、长期后台 8），避免不同模块按各自参数竞争首次初始化而得到
    /// 不可预测的契约。
    /// 用于没有 Tokio 上下文的平台适配代码；已有运行时上下文的调用方应优先使用
    /// [`Self::current`]。创建失败时返回 `None`，调用方应降级并记录警告。
    pub fn shared() -> Option<&'static Self> {
        Self::try_shared().ok()
    }

    /// 获取进程级共享任务运行时，并保留构建失败原因。
    ///
    /// # Errors
    ///
    /// 进程级 Tokio 运行时构建失败时返回 [`TaskRuntimeError::RuntimeBuild`]。
    pub fn try_shared() -> Result<&'static Self, TaskRuntimeError> {
        static SHARED: OnceLock<Result<TaskRuntime, String>> = OnceLock::new();
        SHARED
            .get_or_init(|| {
                Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("snapin-shared")
                    .enable_all()
                    .build()
                    .map(|runtime| {
                        let handle = runtime.handle().clone();
                        Self::from_handle_inner(handle, 16, Some(runtime))
                    })
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|reason| TaskRuntimeError::RuntimeBuild {
                reason: reason.clone(),
            })
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
            Pool::new(4),
            Pool::new(8),
        ];
        Self {
            inner: Arc::new(RuntimeInner {
                handle,
                tracker: TaskTracker::new(),
                pools,
                cancel: CancellationToken::new(),
                shutting_down: AtomicBool::new(false),
                admission: Mutex::new(()),
                next_task_id: AtomicU64::new(1),
                active_tasks: Mutex::new(HashMap::new()),
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
        let _admission = self.lock_admission();
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
        if let Ok(mut active) = self.inner.active_tasks.lock() {
            active.insert(id, abort.clone());
        }
        let inner = Arc::clone(&self.inner);
        // 监视 JoinHandle 以感知任务 panic；监控 Future 本身也纳入 TaskTracker。
        let tracked = self.inner.tracker.track_future(async move {
            if let Err(error) = join.await {
                if error.is_panic() {
                    tracing::error!("后台异步任务发生 panic，任务序号: {id}，已隔离该任务");
                } else {
                    tracing::debug!("后台异步任务在完成前被取消，任务序号: {id}");
                }
            }
            if let Ok(mut active) = inner.active_tasks.lock() {
                active.remove(&id);
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
        let (id, join) = self.spawn_blocking_join(task, Some(pool))?;
        let abort = join.abort_handle();
        self.monitor_blocking_join(id, join);
        Ok(TaskHandle { id, abort })
    }

    /// 在指定池中执行阻塞任务并等待其返回值。
    ///
    /// 与 [`Self::spawn_blocking_in`] 的区别是调用方需要最终结果。阻塞闭包自行持有
    /// 准入许可与追踪令牌，因此调用 Future 被取消后，底层线程仍会被运行时计入停机。
    ///
    /// # Errors
    ///
    /// 运行时停机、目标池已满，或阻塞任务 panic 时返回 [`TaskRuntimeError`]。
    pub async fn run_blocking<F, R>(&self, task: F) -> Result<R, TaskRuntimeError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.run_blocking_in(TaskPool::Default, task).await
    }

    /// 在指定池中执行阻塞任务并等待其返回值。
    ///
    /// # Errors
    ///
    /// 运行时停机、目标池已满，或阻塞任务 panic 时返回 [`TaskRuntimeError`]。
    pub async fn run_blocking_in<F, R>(
        &self,
        pool: TaskPool,
        task: F,
    ) -> Result<R, TaskRuntimeError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (id, join) = self.spawn_blocking_join(task, Some(pool))?;
        let result = join.await;
        if let Ok(mut active) = self.inner.active_tasks.lock() {
            active.remove(&id);
        }
        result.map_err(|error| blocking_task_error(id, error))
    }

    /// 在已持有任务名额的异步任务内派生受追踪的阻塞子任务。
    ///
    /// 媒体任务的外层异步任务已经占用 `Media` 池名额；若在任务内部再次调用
    /// [`Self::spawn_blocking_in`] 申请同池名额，池满时会得到虚假的队列已满错误。
    /// 本入口复用调用方已持有的名额，不再二次准入，但阻塞线程仍登记到
    /// [`TaskTracker`] 与活动任务表，停机等待和 panic 日志保持可见。
    ///
    /// # Errors
    ///
    /// 运行时已停机时返回 [`TaskRuntimeError::ShuttingDown`]。
    pub fn spawn_blocking_detached<F, R>(&self, task: F) -> Result<TaskHandle, TaskRuntimeError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (id, join) = self.spawn_blocking_join(task, None)?;
        let abort = join.abort_handle();
        self.monitor_blocking_join(id, join);
        Ok(TaskHandle { id, abort })
    }

    /// 在已持有任务名额的异步任务内执行并等待阻塞子任务。
    ///
    /// 与 [`Self::run_blocking_in`] 相比不重复申请池名额，适合媒体任务在
    /// 推理、图像编码等 CPU 阶段使用。
    ///
    /// # Errors
    ///
    /// 运行时已停机或阻塞任务 panic 时返回 [`TaskRuntimeError`]。
    pub async fn run_blocking_detached<F, R>(&self, task: F) -> Result<R, TaskRuntimeError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (id, join) = self.spawn_blocking_join(task, None)?;
        let result = join.await;
        if let Ok(mut active) = self.inner.active_tasks.lock() {
            active.remove(&id);
        }
        result.map_err(|error| blocking_task_error(id, error))
    }

    /// 登记并启动一个阻塞任务；`pool` 为 `None` 时复用调用方已持有的名额。
    fn spawn_blocking_join<F, R>(
        &self,
        task: F,
        pool: Option<TaskPool>,
    ) -> Result<(u64, tokio::task::JoinHandle<R>), TaskRuntimeError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let _admission = self.lock_admission();
        if self.inner.shutting_down.load(Ordering::Acquire) {
            return Err(TaskRuntimeError::ShuttingDown);
        }
        let permit = match pool {
            Some(pool) => Some(self.acquire(pool)?),
            None => None,
        };
        let id = self.inner.next_task_id.fetch_add(1, Ordering::Relaxed);
        // 把许可与追踪令牌一起移动进阻塞闭包：即使调用方丢弃 JoinHandle，阻塞线程
        // 完成前仍会占用名额并被 TaskTracker 统计，避免停机误判为已排空。
        let join = self.inner.tracker.spawn_blocking_on(
            move || {
                let _permit = permit;
                task()
            },
            &self.inner.handle,
        );
        if let Ok(mut active) = self.inner.active_tasks.lock() {
            active.insert(id, join.abort_handle());
        }
        Ok((id, join))
    }

    /// 监视阻塞任务完成并清理活动表，panic 时输出告警而不影响其他任务。
    fn monitor_blocking_join<R>(&self, id: u64, join: tokio::task::JoinHandle<R>)
    where
        R: Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        let tracked = self.inner.tracker.track_future(async move {
            match join.await {
                Ok(_) => {}
                Err(error) if error.is_panic() => {
                    tracing::error!("后台阻塞任务发生 panic，任务序号: {id}，已隔离该任务");
                }
                Err(_) => tracing::debug!("后台阻塞任务在完成前被取消，任务序号: {id}"),
            }
            if let Ok(mut active) = inner.active_tasks.lock() {
                active.remove(&id);
            }
        });
        drop(self.inner.handle.spawn(tracked));
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
    /// `Duration::ZERO` 表示只取消并 abort 在途任务、不等待任何收尾；只有运行时本来
    /// 就没有任务时才返回 `true`，不会退化成无限等待。
    ///
    /// 该方法只能在 Tokio 运行时之外或独立运行时中调用；在异步任务内部调用
    /// 会等待自身而永久挂起，调用方必须避免自等待。
    pub async fn shutdown(&self, timeout: Duration) -> bool {
        {
            let _admission = self.lock_admission();
            self.inner.shutting_down.store(true, Ordering::Release);
            self.inner.cancel.cancel();
            self.inner.tracker.close();
        }

        if timeout.is_zero() {
            let stalled_count = abort_active_tasks(&self.inner.active_tasks);
            let drained = stalled_count == 0 && self.inner.tracker.is_empty();
            if !drained {
                tracing::warn!(
                    "后台任务运行时执行零等待停机，仍有 {stalled_count} 个任务未确认退出"
                );
            }
            return drained;
        }

        match tokio::time::timeout(timeout, self.inner.tracker.wait()).await {
            Ok(()) => true,
            Err(_) => {
                let stalled_count = abort_active_tasks(&self.inner.active_tasks);
                tracing::warn!(
                    "等待后台任务退出超时，已等待 {}ms；仍有 {stalled_count} 个任务未响应取消，已请求 abort 并继续退出",
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
    ///
    /// # Panics
    ///
    /// 当前线程已处于 Tokio 运行时上下文时调用会 panic，因为 Tokio 禁止嵌套阻塞等待。
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

    /// 获取准入锁；锁中毒只表示某次登记发生 panic，准入状态本身仍是完整值。
    fn lock_admission(&self) -> MutexGuard<'_, ()> {
        self.inner
            .admission
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// 中止仍在运行的任务并返回中止数量。
fn abort_active_tasks(active_tasks: &Mutex<HashMap<u64, tokio::task::AbortHandle>>) -> usize {
    let stalled: Vec<tokio::task::AbortHandle> = match active_tasks.lock() {
        Ok(mut active) => active.drain().map(|(_, handle)| handle).collect(),
        Err(poisoned) => poisoned
            .into_inner()
            .drain()
            .map(|(_, handle)| handle)
            .collect(),
    };
    let count = stalled.len();
    for handle in stalled {
        handle.abort();
    }
    count
}

/// 将阻塞任务的 JoinError 收敛为运行时领域错误。
fn blocking_task_error(id: u64, error: tokio::task::JoinError) -> TaskRuntimeError {
    TaskRuntimeError::BlockingTaskFailed {
        task_id: id,
        reason: if error.is_panic() {
            "任务发生 panic".to_string()
        } else {
            "任务在完成前被取消".to_string()
        },
    }
}

#[cfg(test)]
mod tests;
