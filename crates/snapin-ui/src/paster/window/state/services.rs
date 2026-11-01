//! 贴图窗口依赖注入：配置快照、媒体执行器与桌面端口。
//!
//! 窗口不自行读配置、不创建线程、不直接访问平台 API；这些能力全部由工作区在
//! 创建窗口时注入。集中为 PasterServices 后，窗口状态只表达“界面是什么样”，
//! 依赖来源与生命周期由应用层掌握。

use crate::paster::window::jobs::PasterJobRuntime;
use crate::ports::DesktopPort;
use snapin_core::config::Config;
use std::sync::Arc;

/// 贴图窗口运行时依赖。
pub(crate) struct PasterServices {
    /// 当前配置快照
    pub(crate) config: Arc<Config>,
    /// 后台媒体任务执行器；测试环境可为空
    pub(crate) job_runtime: Option<Arc<PasterJobRuntime>>,
    /// 桌面副作用端口
    pub(crate) desktop: Arc<dyn DesktopPort>,
}

impl PasterServices {
    /// 用默认配置与无操作端口创建依赖集合，供测试或构造阶段使用。
    pub(crate) fn new(config: Arc<Config>, desktop: Arc<dyn DesktopPort>) -> Self {
        Self {
            config,
            job_runtime: None,
            desktop,
        }
    }
}
