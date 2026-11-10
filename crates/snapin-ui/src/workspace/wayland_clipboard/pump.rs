//! Wayland 原生剪贴板后台事件泵与异步管道传输池。
//!
//! 负责在不与 `winit` / `softbuffer` / `alpha_surface` 争抢 `wl_display` 读锁的前提下，
//! 常驻监听并派发 `wl_data_source` 的 `send` / `cancelled` 与席位输入事件，
//! 并通过独立的阻塞写线程向粘贴端管道发送字节流，避免大截图阻塞事件循环。

use super::dispatch::ClipboardProtoState;
use rustix::event::{poll, PollFd, PollFlags};
use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
use rustix::io::Errno;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use wayland_backend::client::WaylandError;
use wayland_client::{Connection, EventQueue};

/// 允许并发在途的最大剪贴板管道写线程数，防止恶意或异常客户端耗尽线程资源。
const MAX_INFLIGHT_PIPE_WRITES: usize = 8;

/// 与 `ClipboardProtoState` 共存的受保护运行时上下文。
pub(crate) struct WaylandRuntimeState {
    pub(crate) conn: Connection,
    pub(crate) event_queue: EventQueue<ClipboardProtoState>,
    pub(crate) proto: ClipboardProtoState,
}

/// 受管的剪贴板管道写入线程池，确保所有后台写入线程在停机时均可被收割。
#[derive(Default)]
pub(crate) struct PipeWriterPool {
    shutting_down: AtomicBool,
    handles: Mutex<Vec<JoinHandle<()>>>,
}

impl PipeWriterPool {
    /// 提交一次异步剪贴板管道写入；停机中或并发数超限时安全丢弃 `fd` 并记录日志。
    pub(crate) fn submit(&self, bytes: Arc<[u8]>, mime_type: String, fd: OwnedFd) {
        if self.shutting_down.load(Ordering::Acquire) {
            return;
        }
        let mut handles = self
            .handles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        handles.retain(|handle| !handle.is_finished());
        if handles.len() >= MAX_INFLIGHT_PIPE_WRITES {
            tracing::warn!(
                "Wayland 剪贴板管道写入并发数已达上限 ({MAX_INFLIGHT_PIPE_WRITES})，已跳过本次请求: {mime_type}"
            );
            return;
        }

        match std::thread::Builder::new()
            .name("wl-clip-pipe".to_string())
            .spawn(move || write_selection_pipe(&bytes, &mime_type, fd))
        {
            Ok(handle) => handles.push(handle),
            Err(error) => {
                tracing::warn!("启动 Wayland 剪贴板管道写入线程失败: {error}");
            }
        }
    }

    /// 阻止接收新任务并等待所有在途管道写入线程退出。
    pub(crate) fn join_all(&self) {
        self.shutting_down.store(true, Ordering::Release);
        let pending = {
            let mut handles = self
                .handles
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::mem::take(&mut *handles)
        };
        for handle in pending {
            if let Err(payload) = handle.join() {
                tracing::warn!("Wayland 剪贴板管道写入线程异常退出: {payload:?}");
            }
        }
    }
}

/// 清除非阻塞标志并将剪贴板字节完整写入对端管道后关闭 EOF。
fn write_selection_pipe(bytes: &[u8], mime_type: &str, fd: OwnedFd) {
    if let Err(error) = clear_nonblocking(&fd) {
        tracing::warn!("重置 Wayland 剪贴板管道阻塞模式失败，MIME: {mime_type}，原因: {error}");
        return;
    }

    let mut file = File::from(fd);
    match file.write_all(bytes).and_then(|()| file.flush()) {
        Ok(()) => {
            tracing::debug!(
                "已通过 Wayland 原生管道发送剪贴板数据，MIME: {mime_type}，字节数: {}",
                bytes.len()
            );
        }
        Err(error) if error.kind() == ErrorKind::BrokenPipe => {
            tracing::debug!("对端读取前关闭了 Wayland 剪贴板管道，MIME: {mime_type}");
        }
        Err(error) => {
            tracing::warn!("写入 Wayland 剪贴板管道失败，MIME: {mime_type}，原因: {error}");
        }
    }
}

fn clear_nonblocking(fd: &OwnedFd) -> std::io::Result<()> {
    let flags = fcntl_getfl(fd).map_err(std::io::Error::from)?;
    if flags.contains(OFlags::NONBLOCK) {
        fcntl_setfl(fd, flags & !OFlags::NONBLOCK).map_err(std::io::Error::from)?;
    }
    Ok(())
}

/// 泵线程控制状态（与 `WaylandRuntimeState` 锁完全隔离，严禁嵌套加锁）。
#[derive(Default)]
struct ControlState {
    pause_count: usize,
    pump_paused: bool,
    shutdown: bool,
}

/// GUI 主线程与后台 Wayland 事件泵的协调器。
pub(crate) struct PumpControl {
    state: Mutex<ControlState>,
    cv: Condvar,
    wake_wr: OwnedFd,
}

impl PumpControl {
    pub(crate) fn new(wake_wr: OwnedFd) -> Self {
        Self {
            state: Mutex::new(ControlState::default()),
            cv: Condvar::new(),
            wake_wr,
        }
    }

    /// 通知并等待泵线程撤出 `prepare_read()` 进入暂停状态，返回 RAII 恢复守卫。
    pub(crate) fn pause(&self) -> PumpPauseGuard<'_> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pause_count = state.pause_count.saturating_add(1);
        if state.pause_count == 1 && !state.shutdown {
            signal_wake_pipe(&self.wake_wr);
            while !state.pump_paused && !state.shutdown {
                state = self
                    .cv
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        }
        PumpPauseGuard { control: self }
    }

    /// 请求泵线程退出。
    pub(crate) fn request_shutdown(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.shutdown = true;
        self.cv.notify_all();
        drop(state);
        signal_wake_pipe(&self.wake_wr);
    }

    /// 供泵线程在每轮循环开头检查是否需要暂停或停机；返回 `true` 表示应当继续运行。
    fn wait_until_resumed(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if state.shutdown {
                state.pump_paused = true;
                self.cv.notify_all();
                return false;
            }
            if state.pause_count == 0 {
                state.pump_paused = false;
                return true;
            }
            state.pump_paused = true;
            self.cv.notify_all();
            state = self
                .cv
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// 快速检查是否收到暂停或停机请求，避免在即将 `poll` 前遗漏信号。
    fn should_yield_before_poll(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.shutdown || state.pause_count > 0
    }
    /// 在泵线程退出（含异常中断/panic 展开）时置位停止标志并唤醒等待方。
    fn mark_exited(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.shutdown = true;
        state.pump_paused = true;
        self.cv.notify_all();
    }
}

/// 保证泵线程任何退出路径都能解除 GUI 线程暂停等待的 RAII 守卫。
struct PumpExitGuard {
    control: Arc<PumpControl>,
}

impl Drop for PumpExitGuard {
    fn drop(&mut self) {
        self.control.mark_exited();
    }
}

/// 暂停后台 Wayland 事件泵的 RAII 守卫，析构时自动恢复泵线程。
pub(crate) struct PumpPauseGuard<'a> {
    control: &'a PumpControl,
}

impl Drop for PumpPauseGuard<'_> {
    fn drop(&mut self) {
        let mut state = self
            .control
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pause_count = state.pause_count.saturating_sub(1);
        if state.pause_count == 0 {
            self.control.cv.notify_all();
        }
    }
}

/// 后台 Wayland 事件泵主循环。
pub(crate) fn run_pump_loop(
    runtime: Arc<Mutex<Option<WaylandRuntimeState>>>,
    control: Arc<PumpControl>,
    wake_rd: OwnedFd,
) {
    let _exit_guard = PumpExitGuard {
        control: Arc::clone(&control),
    };
    while control.wait_until_resumed() {
        let read_guard = {
            let mut lock = runtime
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(rt) = lock.as_mut() else {
                break;
            };
            if let Err(error) = rt.event_queue.dispatch_pending(&mut rt.proto) {
                tracing::warn!("派发 Wayland 剪贴板待处理事件失败，泵线程退出: {error}");
                break;
            }
            flush_ignoring_wouldblock(&rt.conn);
            rt.event_queue.prepare_read()
        };

        let Some(guard) = read_guard else {
            // 队列仍有待派发事件，立即进入下一轮派发。
            continue;
        };

        if control.should_yield_before_poll() {
            drop(guard);
            drain_wake_pipe(&wake_rd);
            continue;
        }

        let display_ready = {
            let mut fds = [
                PollFd::from_borrowed_fd(guard.connection_fd(), PollFlags::IN),
                PollFd::from_borrowed_fd(wake_rd.as_fd(), PollFlags::IN),
            ];
            match poll(&mut fds, -1) {
                Ok(_) => {
                    let revents = fds[0].revents();
                    if revents.intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL) {
                        tracing::debug!("Wayland 剪贴板事件泵检测到连接断开: {revents:?}");
                        drop(guard);
                        break;
                    }
                    revents.contains(PollFlags::IN)
                }
                Err(Errno::INTR) => false,
                Err(error) => {
                    tracing::warn!("轮询 Wayland 剪贴板事件失败，泵线程退出: {error}");
                    drop(guard);
                    break;
                }
            }
        };

        if display_ready {
            if let Err(error) = guard.read() {
                if !is_wouldblock_wayland(&error) {
                    tracing::warn!("读取 Wayland 剪贴板套接字事件失败，泵线程退出: {error}");
                    break;
                }
            }
            let mut lock = runtime
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(rt) = lock.as_mut() {
                if let Err(error) = rt.event_queue.dispatch_pending(&mut rt.proto) {
                    tracing::warn!("派发 Wayland 剪贴板新事件失败，泵线程退出: {error}");
                    break;
                }
                flush_ignoring_wouldblock(&rt.conn);
            } else {
                break;
            }
        } else {
            drop(guard);
        }

        drain_wake_pipe(&wake_rd);
    }
}

fn flush_ignoring_wouldblock(conn: &Connection) {
    if let Err(error) = conn.flush() {
        if !is_wouldblock_wayland(&error) {
            tracing::debug!("冲刷 Wayland 剪贴板连接失败: {error}");
        }
    }
}

pub(super) fn is_wouldblock_wayland(error: &WaylandError) -> bool {
    matches!(error, WaylandError::Io(io_err) if io_err.kind() == ErrorKind::WouldBlock)
}

fn signal_wake_pipe(wake_wr: &OwnedFd) {
    let _ = rustix::io::write(wake_wr.as_fd(), &[1u8]);
}

fn drain_wake_pipe(wake_rd: &OwnedFd) {
    let mut buf = [0u8; 64];
    loop {
        match rustix::io::read(wake_rd.as_fd(), &mut buf) {
            Ok(0) | Err(Errno::AGAIN | Errno::INTR) => break,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
}
