//! 守护进程停机协调：GUI 握手、IPC 监听收尾与任务运行时退出的统一入口。
//!
//! # 设计原理
//!
//! 停机涉及三个生命周期不同的对象：winit GUI 线程、IPC 监听任务、统一任务运行时。
//! 如果让 `daemon/mod.rs` 逐段 `await`，任何一段没有上限都会把整个进程拖死。
//! 本协调器集中持有各段超时，并保证每一步失败都只记录告警、继续尝试收尾。
//!
//! # 代价与局限
//!
//! winit 事件循环只能由主线程退出；若 GUI 线程被同步系统调用卡死，协调器无法
//! 从服务线程强制打断它，只能依赖超时后主线程自身返回。这里至少保证服务侧
//! 不会无限等待 GUI 确认。

use crate::services::AppServices;
use snapin_ui::workspace::PasterWorkspaceProxy;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// 各收尾阶段的超时配置。
#[derive(Debug, Clone, Copy)]
pub(super) struct ShutdownTimeouts {
    /// 等待 GUI 确认停机
    pub gui: Duration,
    /// 等待 IPC 监听任务退出
    pub listener: Duration,
    /// 等待后台任务运行时排空
    pub runtime: Duration,
}

impl Default for ShutdownTimeouts {
    fn default() -> Self {
        Self {
            gui: Duration::from_secs(2),
            listener: Duration::from_secs(5),
            runtime: Duration::from_secs(5),
        }
    }
}

/// 守护进程停机协调器。
pub(super) struct ShutdownCoordinator {
    timeouts: ShutdownTimeouts,
}

/// 一次守护进程停机所需的协作对象。
pub(super) struct ShutdownRequest<'a> {
    pub workspace: &'a PasterWorkspaceProxy,
    pub gui_closed: &'a watch::Receiver<bool>,
    pub shutdown_tx: &'a watch::Sender<bool>,
    pub listener_handle: JoinHandle<()>,
    pub services: &'a Arc<AppServices>,
}

impl ShutdownCoordinator {
    /// 使用默认超时构建协调器。
    pub(super) fn new() -> Self {
        Self {
            timeouts: ShutdownTimeouts::default(),
        }
    }

    /// 按固定顺序停止 GUI、IPC 监听与任务运行时。
    pub(super) async fn shutdown(&self, request: ShutdownRequest<'_>) {
        let ShutdownRequest {
            workspace,
            gui_closed,
            shutdown_tx,
            listener_handle,
            services,
        } = request;
        if !*gui_closed.borrow() {
            match tokio::time::timeout(self.timeouts.gui, workspace.shutdown_and_wait()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!("请求 GUI 工作区停机未得到确认，继续收尾后台服务: {error}");
                }
                Err(_) => {
                    tracing::warn!(
                        "等待 GUI 工作区停机确认超时（{} 秒），继续收尾后台服务",
                        self.timeouts.gui.as_secs()
                    );
                }
            }
        }

        let _ = shutdown_tx.send(true);
        match tokio::time::timeout(self.timeouts.listener, listener_handle).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!("IPC 监听任务异常退出: {error}"),
            Err(_) => {
                tracing::warn!(
                    "等待 IPC 监听任务退出超时（{} 秒），继续收尾后台运行时",
                    self.timeouts.listener.as_secs()
                );
            }
        }

        // ONNX 推理等阻塞任务无法被强制中断，必须给运行时排空设置上限。
        services.runtime().shutdown(self.timeouts.runtime).await;
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
