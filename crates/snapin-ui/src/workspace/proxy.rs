//! 贴图工作区事件定义与线程安全代理句柄。

use crate::error::UiError;
use crate::model::PasterItem;
use crate::model::{GroupId, PasterId};
use crate::ports::{
    ClipboardJobId, ClipboardResult, ClipboardTarget, JobId, MediaJobOutcome, PasterJobKind,
    PortError, RecordingEvent,
};
use crate::snipper::window::jobs::SnipperJobKind;
use crate::snipper::SnipperLaunch;
use async_trait::async_trait;
use image::RgbaImage;
use snapin_config::config::Config;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use winit::event_loop::EventLoopProxy;

pub use snapin_ports::{WorkspaceDriver, WorkspaceStatus};

/// 集中贴图工作区支持的用户事件指令
pub enum PasterAppEvent {
    /// 重新加载全局配置并同步给存量贴图
    ReloadConfig(Box<Config>),
    /// 创建并弹出一个新的贴图窗口
    OpenPaster(PasterItem),
    /// 在同一事件循环中打开全屏截图覆盖层
    OpenSnipper(Box<SnipperLaunch>),
    /// 截图覆盖层录屏状态更新
    SnipperRecordingEvent {
        /// 录制后台状态
        event: RecordingEvent,
    },
    /// 录屏秒表与结果页内嵌预览的定时推进信号
    SnipperUiTick,
    /// 截图覆盖层媒体任务完成
    SnipperJobFinished {
        /// 执行器分配的任务 ID
        job_id: JobId,
        /// 任务类型
        kind: SnipperJobKind,
        /// 任务结果
        outcome: MediaJobOutcome,
    },
    /// 截图覆盖层异步抓屏完成；请求标识用于丢弃会话重建后的陈旧结果。
    SnipperScreenCaptured {
        /// 截图会话分配的抓屏请求标识
        request_id: u64,
        /// 完整屏幕帧或可展示的中文错误
        result: Result<RgbaImage, PortError>,
    },
    /// 批量切换当前分组内所有贴图的显隐状态
    ToggleVisibility,
    /// 强制显示当前活动分组，打开贴图工作区
    ShowWorkspace,
    /// 切换活动分组（若为 None 则循环轮转至下一个分组）
    SwitchGroup(Option<GroupId>),
    /// 宿主（托盘、全局热键）产生了新动作，请求 GUI 线程排空动作队列。
    HostWake,
    /// 查询当前工作区运行状态
    GetStatus(oneshot::Sender<WorkspaceStatus>),
    /// 贴图后台媒体任务完成；携带 `paster_id` 以精确路由回发起窗口。
    PasterJobFinished {
        /// 发起任务的贴图窗口
        paster_id: PasterId,
        /// 执行器分配的任务 ID
        job_id: JobId,
        /// 任务类别，用于校验结果是否匹配在途任务
        kind: PasterJobKind,
        /// 统一的后台任务结果
        outcome: MediaJobOutcome,
    },
    /// 剪贴板读写任务完成；按目标路由回截图覆盖层或指定贴图窗口。
    ClipboardFinished {
        /// 发起任务的 UI 所有者
        target: ClipboardTarget,
        /// 宿主分配的剪贴板任务标识
        job_id: ClipboardJobId,
        /// 真实系统剪贴板结果
        result: Result<ClipboardResult, PortError>,
    },
    /// 请求工作区平滑停机，并在事件循环即将退出时回传确认
    Shutdown(Option<oneshot::Sender<()>>),
}

/// 贴图工作区线程安全代理句柄，支持跨线程、跨任务异步发送控制事件
#[derive(Clone)]
pub struct PasterWorkspaceProxy {
    /// macOS 的 winit 事件代理只实现 `Send`、未实现 `Sync`。
    /// 通过互斥锁串行化共享访问，使代理句柄本身满足异步任务所需的 `Send + Sync`，
    /// 且锁只在同步事件投递期间持有，不会跨越任何 `.await`。
    proxy: Arc<Mutex<EventLoopProxy<PasterAppEvent>>>,
}

impl PasterWorkspaceProxy {
    /// 封装 winit 事件循环代理
    pub fn new(proxy: EventLoopProxy<PasterAppEvent>) -> Self {
        Self {
            proxy: Arc::new(Mutex::new(proxy)),
        }
    }

    /// 串行投递工作区事件。
    fn send(&self, event: PasterAppEvent) -> Result<(), String> {
        self.proxy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .send_event(event)
            .map_err(|error| error.to_string())
    }

    /// 请求工作区热重载最新配置
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub fn reload_config(&self, cfg: Config) -> Result<(), UiError> {
        self.send(PasterAppEvent::ReloadConfig(Box::new(cfg)))
            .map_err(|error| UiError::Window {
                message: format!("向贴图工作区投递配置热重载指令失败: {error}"),
            })
    }

    /// 请求工作区打开新贴图窗口
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub fn open_paster(&self, item: PasterItem) -> Result<(), UiError> {
        self.send(PasterAppEvent::OpenPaster(item))
            .map_err(|error| UiError::Window {
                message: format!("向贴图工作区投递打开窗口指令失败: {error}"),
            })
    }

    /// 回传剪贴板任务完成结果。
    ///
    /// # Errors
    ///
    /// 工作区事件循环已关闭或事件投递失败时返回 [`UiError`]。
    pub(crate) fn clipboard_finished(
        &self,
        target: ClipboardTarget,
        job_id: ClipboardJobId,
        result: Result<ClipboardResult, PortError>,
    ) -> Result<(), UiError> {
        self.send(PasterAppEvent::ClipboardFinished {
            target,
            job_id,
            result,
        })
        .map_err(|error| UiError::Window {
            message: format!("向贴图工作区投递剪贴板任务结果失败: {error}"),
        })
    }

    /// 唤醒 GUI 线程处理宿主动作队列。
    ///
    /// 托盘与全局热键回调只负责投递“有动作待处理”的轻量事件，真正的动作分发
    /// 仍在 GUI 线程完成；这样平台回调无需跨线程操作窗口，也不会永久定时轮询。
    pub fn wake_host(&self) {
        if let Err(error) = self.send(PasterAppEvent::HostWake) {
            tracing::debug!("贴图工作区已关闭，宿主唤醒事件未投递: {error}");
        }
    }

    /// 请求工作区平滑停机（不等待确认，供 GUI 线程托盘退出动作使用）。
    pub fn request_shutdown(&self) {
        let _ = self.send(PasterAppEvent::Shutdown(None));
    }

    /// 请求工作区打开全屏截图覆盖层
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub fn open_snipper(&self, launch: SnipperLaunch) -> Result<(), UiError> {
        self.send(PasterAppEvent::OpenSnipper(Box::new(launch)))
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区投递截图覆盖层指令失败: {e}"),
            })
    }

    /// 回传贴图后台媒体任务结果。
    ///
    /// 只暴露一条结果通路，任务类别与统一结果一起送回 GUI 线程，由工作区校验
    /// 在途任务并分派；新增任务类型时无需再扩展事件枚举。
    pub(crate) fn paster_job_finished(
        &self,
        paster_id: PasterId,
        job_id: JobId,
        kind: PasterJobKind,
        outcome: MediaJobOutcome,
    ) -> Result<(), UiError> {
        self.send(PasterAppEvent::PasterJobFinished {
            paster_id,
            job_id,
            kind,
            outcome,
        })
        .map_err(|e| UiError::Window {
            message: format!("向贴图工作区回传后台任务结果失败: {e}"),
        })
    }

    /// 请求批量切换当前组贴图显隐
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub fn toggle_visibility(&self) -> Result<(), UiError> {
        self.send(PasterAppEvent::ToggleVisibility)
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区投递显隐切换指令失败: {e}"),
            })
    }

    /// 请求强制显示当前活动分组的全部贴图。
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭或事件投递失败时返回 [`UiError`]。
    pub fn show_workspace(&self) -> Result<(), UiError> {
        self.send(PasterAppEvent::ShowWorkspace)
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区投递显示指令失败: {e}"),
            })
    }

    /// 请求切换活动贴图分组
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub fn switch_group(&self, group_id: Option<GroupId>) -> Result<(), UiError> {
        self.send(PasterAppEvent::SwitchGroup(group_id))
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区投递分组切换指令失败: {e}"),
            })
    }

    /// 异步获取工作区当前活动状态快照
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub async fn get_status(&self) -> Result<WorkspaceStatus, UiError> {
        let (tx, rx) = oneshot::channel();
        self.send(PasterAppEvent::GetStatus(tx))
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区投递状态查询指令失败: {e}"),
            })?;
        rx.await.map_err(|_| UiError::Window {
            message: "等待贴图工作区响应状态快照超时或主循环已退出".to_string(),
        })
    }

    /// 请求工作区平滑退出，并等待事件循环确认（最多 2 秒）
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub async fn shutdown_and_wait(&self) -> Result<(), UiError> {
        let (sender, receiver) = oneshot::channel();
        self.send(PasterAppEvent::Shutdown(Some(sender)))
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区投递停机指令失败: {e}"),
            })?;

        tokio::time::timeout(std::time::Duration::from_secs(2), receiver)
            .await
            .map_err(|_| UiError::Window {
                message: "等待贴图工作区停机确认超时".to_string(),
            })?
            .map_err(|_| UiError::Window {
                message: "贴图工作区在停机前已退出".to_string(),
            })
    }
}

#[async_trait]
impl WorkspaceDriver for PasterWorkspaceProxy {
    fn open_snipper(&self, launch: SnipperLaunch) -> Result<(), PortError> {
        self.open_snipper(launch)
            .map_err(|e| PortError::new("open_snipper", e.to_string()))
    }

    fn open_paster(&self, item: PasterItem) -> Result<(), PortError> {
        self.open_paster(item)
            .map_err(|e| PortError::new("open_paster", e.to_string()))
    }

    fn reload_config(&self, cfg: Config) -> Result<(), PortError> {
        self.reload_config(cfg)
            .map_err(|e| PortError::new("reload_config", e.to_string()))
    }

    fn wake_host(&self) {
        self.wake_host();
    }

    fn toggle_visibility(&self) -> Result<(), PortError> {
        self.toggle_visibility()
            .map_err(|e| PortError::new("toggle_visibility", e.to_string()))
    }

    fn show_workspace(&self) -> Result<(), PortError> {
        self.show_workspace()
            .map_err(|e| PortError::new("show_workspace", e.to_string()))
    }

    fn switch_group(&self, group_id: Option<GroupId>) -> Result<(), PortError> {
        self.switch_group(group_id)
            .map_err(|e| PortError::new("switch_group", e.to_string()))
    }

    async fn get_status(&self) -> Result<WorkspaceStatus, PortError> {
        self.get_status()
            .await
            .map_err(|e| PortError::new("get_status", e.to_string()))
    }

    fn request_shutdown(&self) {
        self.request_shutdown();
    }

    async fn shutdown_and_wait(&self) -> Result<(), PortError> {
        self.shutdown_and_wait()
            .await
            .map_err(|e| PortError::new("shutdown_and_wait", e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 工作区代理必须可跨线程共享，保证异步任务在 macOS 上满足 Tokio `Send` 约束。
    #[test]
    fn test_workspace_proxy_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PasterWorkspaceProxy>();
    }
}
