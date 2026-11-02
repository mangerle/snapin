//! 守护主循环的事件源与平台信号等待。

use snapin_ui::snipper::SnipperOutcome;
#[cfg(unix)]
use tokio::signal::unix::Signal;
use tokio::sync::mpsc;
use tokio::sync::watch;

/// 守护主循环可选择处理的事件。
pub(super) enum DaemonEvent {
    Shutdown,
    GuiClosed,
    Snipper(SnipperOutcome),
    Noop,
    Ipc(snapin_daemon::IpcInvocation),
    IpcClosed,
}

/// 守护主循环的输入通道与平台信号源。
pub(super) struct DaemonEventSources {
    ipc_rx: mpsc::Receiver<snapin_daemon::IpcInvocation>,
    snipper_rx: mpsc::Receiver<SnipperOutcome>,
    gui_closed: watch::Receiver<bool>,
    snipper_channel_open: bool,
    #[cfg(unix)]
    sigterm: Signal,
}

impl DaemonEventSources {
    /// 组装守护主循环的输入通道与平台信号。
    pub(super) fn new(
        ipc_rx: mpsc::Receiver<snapin_daemon::IpcInvocation>,
        snipper_rx: mpsc::Receiver<SnipperOutcome>,
        gui_closed: watch::Receiver<bool>,
        #[cfg(unix)] sigterm: Signal,
    ) -> Self {
        Self {
            ipc_rx,
            snipper_rx,
            gui_closed,
            snipper_channel_open: true,
            #[cfg(unix)]
            sigterm,
        }
    }

    /// 取出 GUI 停机状态接收端，供收尾协调器等待确认。
    pub(super) fn into_gui_closed(self) -> watch::Receiver<bool> {
        self.gui_closed
    }

    /// 等待并返回下一个需要处理的生命周期或业务事件。
    pub(super) async fn next_event(&mut self) -> DaemonEvent {
        let ipc_rx = &mut self.ipc_rx;
        let snipper_rx = &mut self.snipper_rx;
        let gui_closed = &mut self.gui_closed;
        let snipper_channel_open = &mut self.snipper_channel_open;
        #[cfg(unix)]
        let sigterm = &mut self.sigterm;

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("收到中断信号 (Ctrl+C)，开始平滑停机");
                DaemonEvent::Shutdown
            }
            _ = async {
                #[cfg(unix)]
                {
                    sigterm.recv().await
                }
                #[cfg(not(unix))]
                {
                    std::future::pending::<Option<()>>().await
                }
            } => {
                tracing::info!("收到 SIGTERM，开始平滑停机");
                DaemonEvent::Shutdown
            }
            changed = gui_closed.changed() => {
                if changed.is_err() || *gui_closed.borrow() {
                    DaemonEvent::GuiClosed
                } else {
                    DaemonEvent::Noop
                }
            }
            maybe_outcome = snipper_rx.recv(), if *snipper_channel_open => {
                match maybe_outcome {
                    Some(outcome) => DaemonEvent::Snipper(outcome),
                    None => {
                        *snipper_channel_open = false;
                        DaemonEvent::Noop
                    }
                }
            }
            maybe_invocation = ipc_rx.recv() => {
                match maybe_invocation {
                    Some(invocation) => DaemonEvent::Ipc(invocation),
                    None => DaemonEvent::IpcClosed,
                }
            }
        }
    }
}
