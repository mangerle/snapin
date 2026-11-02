//! 贴图工作区事件定义与线程安全代理句柄。

use crate::error::UiError;
use crate::model::PasterItem;
use crate::model::{GroupId, PasterId};
use crate::ports::{JobId, MediaJobOutcome};
use crate::snipper::window::jobs::SnipperJobKind;
use crate::snipper::SnipperLaunch;
use snapin_annotation::shapes::TranslationShapeItem;
use snapin_config::config::Config;
use std::path::PathBuf;
use tokio::sync::oneshot;
use winit::event_loop::EventLoopProxy;

/// 工作区当前状态快照
#[derive(Debug, Clone)]
pub struct WorkspaceStatus {
    pub active_group: GroupId,
    pub paster_count: usize,
}

/// 集中贴图工作区支持的用户事件指令
pub enum PasterAppEvent {
    /// 重新加载全局配置并同步给存量贴图
    ReloadConfig(Box<Config>),
    /// 创建并弹出一个新的贴图窗口
    OpenPaster(PasterItem),
    /// 在同一事件循环中打开全屏截图覆盖层
    OpenSnipper(Box<SnipperLaunch>),
    /// 截图覆盖层媒体任务完成
    SnipperJobFinished {
        /// 执行器分配的任务 ID
        job_id: JobId,
        /// 任务类型
        kind: SnipperJobKind,
        /// 任务结果
        outcome: MediaJobOutcome,
    },
    /// 批量切换当前分组内所有贴图的显隐状态
    ToggleVisibility,
    /// 切换活动分组（若为 None 则循环轮转至下一个分组）
    SwitchGroup(Option<GroupId>),
    /// 查询当前工作区运行状态
    GetStatus(oneshot::Sender<WorkspaceStatus>),
    /// 整图文字识别完成；携带 `paster_id` 以精确路由回发起窗口
    OcrFinished {
        paster_id: PasterId,
        job_id: JobId,
        result: Result<String, String>,
    },
    /// 整图翻译完成；携带 `paster_id` 以精确路由回发起窗口
    TranslateFinished {
        paster_id: PasterId,
        job_id: JobId,
        result: Result<Vec<TranslationShapeItem>, String>,
    },
    /// 贴图保存完成：成功且用户选择路径时返回目标文件，取消时返回 `None`
    SaveFinished {
        paster_id: PasterId,
        job_id: JobId,
        result: Result<Option<PathBuf>, String>,
    },
    /// 请求工作区平滑停机，并在事件循环即将退出时回传确认
    Shutdown(Option<oneshot::Sender<()>>),
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

    /// 请求工作区热重载最新配置
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub fn reload_config(&self, cfg: Config) -> Result<(), UiError> {
        self.proxy
            .send_event(PasterAppEvent::ReloadConfig(Box::new(cfg)))
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区投递配置热重载指令失败: {e}"),
            })
    }

    /// 请求工作区打开新贴图窗口
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub fn open_paster(&self, item: PasterItem) -> Result<(), UiError> {
        self.proxy
            .send_event(PasterAppEvent::OpenPaster(item))
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区投递打开窗口指令失败: {e}"),
            })
    }

    /// 请求工作区平滑停机（不等待确认，供 GUI 线程托盘退出动作使用）。
    pub fn request_shutdown(&self) {
        let _ = self.proxy.send_event(PasterAppEvent::Shutdown(None));
    }

    /// 请求工作区打开全屏截图覆盖层
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub fn open_snipper(&self, launch: SnipperLaunch) -> Result<(), UiError> {
        self.proxy
            .send_event(PasterAppEvent::OpenSnipper(Box::new(launch)))
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区投递截图覆盖层指令失败: {e}"),
            })
    }

    /// 回传整图文字识别结果。
    pub(crate) fn ocr_finished(
        &self,
        paster_id: PasterId,
        job_id: JobId,
        result: Result<String, String>,
    ) -> Result<(), UiError> {
        self.proxy
            .send_event(PasterAppEvent::OcrFinished {
                paster_id,
                job_id,
                result,
            })
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区回传文字识别结果失败: {e}"),
            })
    }

    /// 回传整图翻译结果。
    pub(crate) fn translate_finished(
        &self,
        paster_id: PasterId,
        job_id: JobId,
        result: Result<Vec<TranslationShapeItem>, String>,
    ) -> Result<(), UiError> {
        self.proxy
            .send_event(PasterAppEvent::TranslateFinished {
                paster_id,
                job_id,
                result,
            })
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区回传翻译结果失败: {e}"),
            })
    }

    /// 回传贴图保存结果，供工作区更新对应贴图的文件路径与状态提示。
    pub(crate) fn save_finished(
        &self,
        paster_id: PasterId,
        job_id: JobId,
        result: Result<Option<PathBuf>, String>,
    ) -> Result<(), UiError> {
        self.proxy
            .send_event(PasterAppEvent::SaveFinished {
                paster_id,
                job_id,
                result,
            })
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区回传保存结果失败: {e}"),
            })
    }

    /// 请求批量切换当前组贴图显隐
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub fn toggle_visibility(&self) -> Result<(), UiError> {
        self.proxy
            .send_event(PasterAppEvent::ToggleVisibility)
            .map_err(|e| UiError::Window {
                message: format!("向贴图工作区投递显隐切换指令失败: {e}"),
            })
    }

    /// 请求切换活动贴图分组
    ///
    /// # Errors
    ///
    /// 事件循环代理已关闭、投递失败或等待响应超时时返回 [`UiError`]。
    pub fn switch_group(&self, group_id: Option<GroupId>) -> Result<(), UiError> {
        self.proxy
            .send_event(PasterAppEvent::SwitchGroup(group_id))
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
        self.proxy
            .send_event(PasterAppEvent::GetStatus(tx))
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
        self.proxy
            .send_event(PasterAppEvent::Shutdown(Some(sender)))
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
