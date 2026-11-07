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

    /// 任务内阻塞子任务复用外层已持有的名额，不得因同池已满被拒绝。
    #[tokio::test]
    async fn test_run_blocking_detached_reuses_outer_permit() {
        let runtime = TaskRuntime::current(1).expect("测试运行时必须可用");
        let task_runtime = runtime.clone();
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        runtime
            .spawn(async move {
                let value = task_runtime
                    .run_blocking_detached(|| 7_u32)
                    .await
                    .expect("任务内阻塞子任务应成功");
                let _ = result_tx.send(value);
            })
            .expect("外层任务应成功入队");

        assert_eq!(result_rx.await.expect("任务应回传结果"), 7);
        assert!(runtime.shutdown(Duration::from_secs(1)).await);
    }

    /// 可等待的阻塞任务必须返回闭包结果，并在完成后释放池名额。    /// 可等待的阻塞任务必须返回闭包结果，并在完成后释放池名额。
    #[tokio::test]
    async fn test_run_blocking_returns_value_and_releases_permit() {
        let runtime = TaskRuntime::current(1).expect("测试运行时必须可用");

        let first = runtime
            .run_blocking_in(TaskPool::Maintenance, || 41)
            .await
            .expect("首个阻塞任务应成功");
        let second = runtime
            .run_blocking_in(TaskPool::Maintenance, move || first + 1)
            .await
            .expect("完成后名额应被释放");

        assert_eq!(second, 42);
        assert!(runtime.shutdown(Duration::from_secs(1)).await);
    }

    /// 阻塞任务 panic 必须转换为强类型错误，供调用方安全降级。
    #[tokio::test]
    async fn test_run_blocking_maps_panic_to_domain_error() {
        let runtime = TaskRuntime::current(1).expect("测试运行时必须可用");
        let result = runtime
            .run_blocking(|| -> u8 { panic!("模拟阻塞任务 panic") })
            .await;

        assert!(matches!(
            result,
            Err(TaskRuntimeError::BlockingTaskFailed { .. })
        ));
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

    /// 零等待停机必须立即 abort 并返回未排空，而不是无限等待。
    #[tokio::test]
    async fn test_zero_timeout_shutdown_does_not_block() {
        let runtime = TaskRuntime::current(1).expect("测试运行时必须可用");
        runtime
            .spawn(async {
                tokio::time::sleep(Duration::from_secs(30)).await;
            })
            .expect("任务应成功入队");

        let drained = runtime.shutdown(Duration::ZERO).await;
        assert!(!drained, "有在途任务时零等待停机必须返回未排空");
    }

    /// 停机超时后必须主动 abort 未退出任务，而不是让其在运行时外继续存活。
    #[tokio::test]
    async fn test_shutdown_timeout_aborts_remaining_tasks() {
        struct DropFlag(Arc<AtomicBool>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }

        let runtime = TaskRuntime::current(1).expect("测试运行时必须可用");
        let dropped = Arc::new(AtomicBool::new(false));
        let flag = dropped.clone();
        runtime
            .spawn(async move {
                let _guard = DropFlag(flag);
                tokio::time::sleep(Duration::from_secs(30)).await;
            })
            .expect("任务应成功入队");

        assert!(!runtime.shutdown(Duration::from_millis(10)).await);
        tokio::task::yield_now().await;
        assert!(
            dropped.load(Ordering::Acquire),
            "超时停机必须 abort 并丢弃未退出异步任务"
        );
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

    /// 共享运行时构建结果必须可被调用方显式观察，而不是只得到一个无原因的 None。
    #[test]
    fn test_try_shared_returns_cached_runtime() {
        let first = TaskRuntime::try_shared().expect("共享运行时首次构建应成功");
        let second = TaskRuntime::try_shared().expect("共享运行时后续应直接复用");
        assert!(std::ptr::eq(first, second));
    }
