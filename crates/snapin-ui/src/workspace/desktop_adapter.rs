//! 工作区桌面端口适配器：在 Linux Wayland 环境下优先通过原生协议写入剪贴板。
//!
//! # 设计原理
//!
//! 将原生 Wayland 剪贴板写入封装在 [`DesktopPort`] 适配层内，使截图覆盖层与贴图窗口
//! 无需感知底层协议差异，也不影响 `snapin-ports` 与 `snapin-app` 的跨平台端口契约。
//! 当处于非 Wayland 会话或当前窗口无焦点/无有效输入序列号时，透明回退至宿主传入的
//! `arboard` 桌面端口实现。

use super::wayland_clipboard::WaylandClipboard;
#[cfg(target_os = "linux")]
use super::wayland_clipboard::{PreparedSelection, SelectionTarget};
use crate::ports::DesktopPort;
#[cfg(target_os = "linux")]
use crate::ports::{
    ClipboardJobId, ClipboardResult, ClipboardSink, ClipboardWrite, PortError, PortErrorKind,
    ScreenCaptureSink, WheelDelta,
};
#[cfg(target_os = "linux")]
use snapin_foundation::Rect;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "linux")]
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::sync::Mutex;
#[cfg(target_os = "linux")]
use std::thread::JoinHandle;

/// Wayland 写入异步工作队列容量；与应用层 `AppDesktop` 队列容量保持一致。
#[cfg(target_os = "linux")]
const WAYLAND_WRITE_QUEUE_CAPACITY: usize = 8;
/// 本地分配任务序号的高位掩码，确保与宿主 `AppDesktop` 从 1 递增的任务序号永不重叠。
#[cfg(target_os = "linux")]
const WAYLAND_JOB_ID_BASE: u64 = 1_u64 << 63;

/// 若存在有效的 Wayland 原生剪贴板写入器，则返回优先走原生协议的适配端口；否则原样返回宿主端口。
#[cfg(target_os = "linux")]
pub(crate) fn wrap_desktop_with_wayland(
    inner: Arc<dyn DesktopPort>,
    wayland: Option<&Arc<WaylandClipboard>>,
) -> Arc<dyn DesktopPort> {
    match wayland {
        Some(wayland) => WorkspaceDesktop::wrap_or_fallback(inner, Arc::clone(wayland)),
        None => inner,
    }
}

/// 一次性剪贴板完成回执守卫，确保任意路径均严格回调一次 `sink`。
#[cfg(target_os = "linux")]
struct JobCompletion {
    job_id: ClipboardJobId,
    sink: Option<ClipboardSink>,
}

#[cfg(target_os = "linux")]
impl JobCompletion {
    fn complete(&mut self, result: Result<ClipboardResult, PortError>) {
        if let Some(sink) = self.sink.take() {
            sink(self.job_id, result);
        }
    }

    fn disarm(&mut self) {
        self.sink = None;
    }
}

#[cfg(target_os = "linux")]
impl Drop for JobCompletion {
    fn drop(&mut self) {
        self.complete(Err(PortError::with_kind(
            PortErrorKind::Other,
            "执行 Wayland 原生剪贴板写入任务",
            "剪贴板写入任务在返回结果前异常退出",
        )));
    }
}

#[cfg(target_os = "linux")]
struct WaylandWriteTask {
    target: SelectionTarget,
    write: ClipboardWrite,
    completion: JobCompletion,
}

/// 在 Wayland 环境下优先走 `wl_data_device.set_selection` 的桌面端口包装器。
#[cfg(target_os = "linux")]
pub(crate) struct WorkspaceDesktop {
    inner: Arc<dyn DesktopPort>,
    wayland: Arc<WaylandClipboard>,
    next_job_id: AtomicU64,
    tx: Mutex<Option<SyncSender<WaylandWriteTask>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

#[cfg(target_os = "linux")]
impl WorkspaceDesktop {
    fn wrap_or_fallback(
        inner: Arc<dyn DesktopPort>,
        wayland: Arc<WaylandClipboard>,
    ) -> Arc<dyn DesktopPort> {
        let (tx, rx) = mpsc::sync_channel::<WaylandWriteTask>(WAYLAND_WRITE_QUEUE_CAPACITY);
        let worker_inner = Arc::clone(&inner);
        let worker_wayland = Arc::clone(&wayland);
        let handle = std::thread::Builder::new()
            .name("wl-clip-worker".to_string())
            .spawn(move || run_wayland_write_worker(rx, worker_inner, worker_wayland));

        match handle {
            Ok(handle) => Arc::new(Self {
                inner,
                wayland,
                next_job_id: AtomicU64::new(1),
                tx: Mutex::new(Some(tx)),
                worker: Mutex::new(Some(handle)),
            }),
            Err(error) => {
                tracing::warn!("启动 Wayland 剪贴板编码线程失败，回退宿主桌面端口: {error}");
                inner
            }
        }
    }

    fn allocate_job_id(&self) -> ClipboardJobId {
        let seq = self.next_job_id.fetch_add(1, Ordering::Relaxed);
        ClipboardJobId::new(WAYLAND_JOB_ID_BASE | seq)
    }
}

#[cfg(target_os = "linux")]
impl Drop for WorkspaceDesktop {
    fn drop(&mut self) {
        // 先销毁发送端使工作线程自然排空并退出，再同步回收线程句柄。
        let sender = self
            .tx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        drop(sender);
        if let Some(handle) = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            if let Err(payload) = handle.join() {
                tracing::warn!("Wayland 剪贴板编码线程异常退出: {payload:?}");
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run_wayland_write_worker(
    rx: mpsc::Receiver<WaylandWriteTask>,
    inner: Arc<dyn DesktopPort>,
    wayland: Arc<WaylandClipboard>,
) {
    while let Ok(mut task) = rx.recv() {
        let prepared = match PreparedSelection::from_write(&task.write) {
            Ok(prepared) => prepared,
            Err(message) => {
                task.completion.complete(Err(PortError::with_kind(
                    PortErrorKind::InvalidInput,
                    "编码 Wayland 剪贴板载荷",
                    message,
                )));
                continue;
            }
        };

        if wayland.publish_selection(task.target, prepared) {
            task.completion.complete(Ok(ClipboardResult::Written));
        } else {
            fallback_to_inner(&inner, task.write, task.completion);
        }
    }
}

#[cfg(target_os = "linux")]
fn fallback_to_inner(
    inner: &Arc<dyn DesktopPort>,
    write: ClipboardWrite,
    completion: JobCompletion,
) {
    let shared = Arc::new(Mutex::new(Some(completion)));
    let proxy_shared = Arc::clone(&shared);
    let proxy_sink: ClipboardSink = Box::new(move |_inner_id, result| {
        if let Some(mut comp) = proxy_shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            comp.complete(result);
        }
    });
    if let Err(error) = inner.write_clipboard(write, proxy_sink) {
        if let Some(mut comp) = shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            comp.complete(Err(error));
        }
    }
}

#[cfg(target_os = "linux")]
impl DesktopPort for WorkspaceDesktop {
    fn load_last_selection(&self) -> Option<Rect> {
        self.inner.load_last_selection()
    }

    fn save_last_selection(&self, rect: Rect) {
        self.inner.save_last_selection(rect);
    }

    fn write_clipboard(
        &self,
        write: ClipboardWrite,
        sink: ClipboardSink,
    ) -> Result<ClipboardJobId, PortError> {
        let Some(target) = self.wayland.capture_selection_target() else {
            tracing::debug!("当前无有效 Wayland 聚焦输入序列号，本次剪贴板写入回退至宿主通道");
            return self.inner.write_clipboard(write, sink);
        };

        let job_id = self.allocate_job_id();
        let mut task = WaylandWriteTask {
            target,
            write,
            completion: JobCompletion {
                job_id,
                sink: Some(sink),
            },
        };

        let send_result = {
            let tx_guard = self
                .tx
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match tx_guard.as_ref() {
                Some(tx) => tx.try_send(task),
                None => {
                    task.completion.disarm();
                    return Err(PortError::with_kind(
                        PortErrorKind::Unavailable,
                        "提交 Wayland 剪贴板写入任务",
                        "工作区剪贴板通道已关闭",
                    ));
                }
            }
        };

        match send_result {
            Ok(()) => Ok(job_id),
            Err(TrySendError::Full(mut rejected)) => {
                rejected.completion.disarm();
                Err(PortError::with_kind(
                    PortErrorKind::Busy,
                    "提交 Wayland 剪贴板写入任务",
                    "Wayland 剪贴板写入队列已满",
                ))
            }
            Err(TrySendError::Disconnected(mut rejected)) => {
                let write = rejected.write;
                rejected.completion.disarm();
                Err(PortError::with_kind(
                    PortErrorKind::Unavailable,
                    "提交 Wayland 剪贴板写入任务",
                    format!(
                        "Wayland 剪贴板写入线程已停止（载荷类型: {}）",
                        match write {
                            ClipboardWrite::Text(_) => "文本",
                            ClipboardWrite::Image(_) => "图像",
                        }
                    ),
                ))
            }
        }
    }

    fn read_clipboard(&self, sink: ClipboardSink) -> Result<ClipboardJobId, PortError> {
        self.inner.read_clipboard(sink)
    }

    fn open_in_file_manager(&self, path: PathBuf) {
        self.inner.open_in_file_manager(path);
    }

    fn open_url(&self, url: &str) -> Result<(), PortError> {
        self.inner.open_url(url)
    }

    fn notify_copy(&self, body: &str) {
        self.inner.notify_copy(body);
    }

    fn capture_long_capture_frame(
        &self,
        delta: WheelDelta,
        sink: ScreenCaptureSink,
    ) -> Result<(), PortError> {
        self.inner.capture_long_capture_frame(delta, sink)
    }
}

/// 非 Linux 平台直接透传宿主桌面端口，不引入任何 Wayland 开销。
#[cfg(not(target_os = "linux"))]
pub(crate) fn wrap_desktop_with_wayland(
    inner: Arc<dyn DesktopPort>,
    _wayland: Option<&Arc<WaylandClipboard>>,
) -> Arc<dyn DesktopPort> {
    inner
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::ports::NoopDesktop;

    struct FailingWriteDesktop;

    impl DesktopPort for FailingWriteDesktop {
        fn load_last_selection(&self) -> Option<Rect> {
            None
        }

        fn save_last_selection(&self, _rect: Rect) {}

        fn write_clipboard(
            &self,
            _write: ClipboardWrite,
            _sink: ClipboardSink,
        ) -> Result<ClipboardJobId, PortError> {
            Err(PortError::with_kind(
                PortErrorKind::Unavailable,
                "写入剪贴板",
                "模拟宿主剪贴板不可用",
            ))
        }

        fn read_clipboard(&self, sink: ClipboardSink) -> Result<ClipboardJobId, PortError> {
            NoopDesktop.read_clipboard(sink)
        }

        fn open_in_file_manager(&self, _path: PathBuf) {}

        fn open_url(&self, _url: &str) -> Result<(), PortError> {
            Ok(())
        }

        fn notify_copy(&self, _body: &str) {}
    }

    #[test]
    fn test_wrap_desktop_without_wayland_returns_original_port() {
        let inner: Arc<dyn DesktopPort> = Arc::new(NoopDesktop);
        let wrapped = wrap_desktop_with_wayland(Arc::clone(&inner), None);
        assert!(Arc::ptr_eq(&inner, &wrapped));
    }

    #[test]
    fn test_fallback_to_inner_preserves_caller_job_id_on_success() {
        let inner: Arc<dyn DesktopPort> = Arc::new(NoopDesktop);
        let expected_id = ClipboardJobId::new(WAYLAND_JOB_ID_BASE | 42);
        let captured = Arc::new(Mutex::new(Vec::new()));
        let sink_captured = Arc::clone(&captured);

        let completion = JobCompletion {
            job_id: expected_id,
            sink: Some(Box::new(move |id, res| {
                sink_captured.lock().unwrap().push((id, res.is_ok()));
            })),
        };
        fallback_to_inner(&inner, ClipboardWrite::Text("abc".to_string()), completion);

        let events = captured.lock().unwrap();
        assert_eq!(*events, vec![(expected_id, true)]);
    }

    #[test]
    fn test_fallback_to_inner_propagates_sync_submit_error_once() {
        let inner: Arc<dyn DesktopPort> = Arc::new(FailingWriteDesktop);
        let expected_id = ClipboardJobId::new(WAYLAND_JOB_ID_BASE | 99);
        let captured = Arc::new(Mutex::new(Vec::new()));
        let sink_captured = Arc::clone(&captured);

        let completion = JobCompletion {
            job_id: expected_id,
            sink: Some(Box::new(move |id, res| {
                sink_captured.lock().unwrap().push((id, res.is_err()));
            })),
        };
        fallback_to_inner(&inner, ClipboardWrite::Text("abc".to_string()), completion);

        let events = captured.lock().unwrap();
        assert_eq!(*events, vec![(expected_id, true)]);
    }
}
