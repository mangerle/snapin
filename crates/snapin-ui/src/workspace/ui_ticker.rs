//! 录屏界面秒表与结果页预览的定时唤醒器。
//!
//! Wayland 下窗口静止时，仅依赖 `ControlFlow::WaitUntil` 无法保证按短间隔回到
//! `about_to_wait`，导致秒表或预览动画只能在下一次输入事件后推进。这里使用一个
//! 受管线程定期投递有界频率的 UI 事件；线程在结构体析构时置停止标记并等待退出，
//! 避免遗留孤儿任务。

use super::PasterAppEvent;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use winit::event_loop::EventLoopProxy;

const UI_TICK_INTERVAL: Duration = Duration::from_millis(40);

/// 录屏秒表与结果页预览的定时唤醒器。
pub(crate) struct RecordingUiTicker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl RecordingUiTicker {
    /// 启动定时唤醒线程。
    ///
    /// # Errors
    ///
    /// 系统拒绝创建线程时返回中文错误。
    pub(super) fn start(proxy: EventLoopProxy<PasterAppEvent>) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let handle = thread::Builder::new()
            .name("snapin-recording-ui".to_string())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    thread::sleep(UI_TICK_INTERVAL);
                    if thread_stop.load(Ordering::Acquire) {
                        break;
                    }
                    if proxy.send_event(PasterAppEvent::SnipperUiTick).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("创建录屏界面定时线程失败: {error}"))?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for RecordingUiTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            if handle.join().is_err() {
                tracing::warn!("录屏界面定时线程退出时发生异常");
            }
        }
    }
}
