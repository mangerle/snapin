//! 贴图工作区事件定义与线程安全代理句柄。

use super::super::manager::PasterItem;
use snapin_core::error::SnapinError;
use tokio::sync::oneshot;
use winit::event_loop::EventLoopProxy;

/// 工作区当前状态快照
#[derive(Debug, Clone)]
pub struct WorkspaceStatus {
    pub active_group: usize,
    pub paster_count: usize,
}

/// 集中贴图工作区支持的用户事件指令
pub enum PasterAppEvent {
    /// 创建并弹出一个新的贴图窗口
    OpenPaster(PasterItem),
    /// 批量切换当前分组内所有贴图的显隐状态
    ToggleVisibility,
    /// 切换活动分组（若为 None 则循环轮转至下一个分组）
    SwitchGroup(Option<usize>),
    /// 显式设置当前活动组所有贴图的可见性
    SetVisibility(bool),
    /// 关闭指定 ID 的贴图
    ClosePaster(u64),
    /// 查询当前工作区运行状态
    GetStatus(oneshot::Sender<WorkspaceStatus>),
    /// 请求工作区平滑停机
    Shutdown,
}

/// 贴图工作区线程安全代理句柄，支持跨线程、跨任务异步发送控制事件
#[derive(Clone)]
pub struct PasterWorkspaceProxy {
    proxy: EventLoopProxy<PasterAppEvent>,
}

impl PasterWorkspaceProxy {
    /// 封装 winit 事件循环代理
    pub fn new(proxy: EventLoopProxy<PasterAppEvent>) -> Self {
        Self { proxy }
    }

    /// 请求工作区打开新贴图窗口
    pub fn open_paster(&self, item: PasterItem) -> Result<(), SnapinError> {
        self.proxy
            .send_event(PasterAppEvent::OpenPaster(item))
            .map_err(|e| SnapinError::WindowError {
                message: format!("向贴图工作区投递打开窗口指令失败: {e}"),
            })
    }

    /// 请求批量切换当前组贴图显隐
    pub fn toggle_visibility(&self) -> Result<(), SnapinError> {
        self.proxy
            .send_event(PasterAppEvent::ToggleVisibility)
            .map_err(|e| SnapinError::WindowError {
                message: format!("向贴图工作区投递显隐切换指令失败: {e}"),
            })
    }

    /// 请求切换活动贴图分组
    pub fn switch_group(&self, group_id: Option<usize>) -> Result<(), SnapinError> {
        self.proxy
            .send_event(PasterAppEvent::SwitchGroup(group_id))
            .map_err(|e| SnapinError::WindowError {
                message: format!("向贴图工作区投递分组切换指令失败: {e}"),
            })
    }

    /// 异步获取工作区当前活动状态快照
    pub async fn get_status(&self) -> Result<WorkspaceStatus, SnapinError> {
        let (tx, rx) = oneshot::channel();
        self.proxy
            .send_event(PasterAppEvent::GetStatus(tx))
            .map_err(|e| SnapinError::WindowError {
                message: format!("向贴图工作区投递状态查询指令失败: {e}"),
            })?;
        rx.await.map_err(|_| SnapinError::WindowError {
            message: "等待贴图工作区响应状态快照超时或主循环已退出".to_string(),
        })
    }

    /// 请求工作区平滑退出
    pub fn shutdown(&self) -> Result<(), SnapinError> {
        self.proxy
            .send_event(PasterAppEvent::Shutdown)
            .map_err(|e| SnapinError::WindowError {
                message: format!("向贴图工作区投递停机指令失败: {e}"),
            })
    }
}
