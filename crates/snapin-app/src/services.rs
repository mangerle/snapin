//! Snapin 应用服务容器：把配置快照、统一后台运行时、桌面端口与媒体执行器装配在一起。
//!
//! 守护进程、单次截图与单次贴图都通过该容器获取宿主能力，避免各自读取配置、
//! 创建线程或重复初始化 OCR/翻译服务。容器本身不依赖 GUI 事件循环，便于在
//! 应用层集成测试中替换端口。

use crate::config::{publish_config, shared_config, SharedConfig};
use crate::desktop::AppDesktop;
use crate::media::MediaService;
use snapin_config::config::Config;
use snapin_ports::{DesktopPort, MediaJobRunner, PortError};
use snapin_runtime::TaskRuntime;
use std::sync::Arc;

/// 应用级服务容器。
pub(crate) struct AppServices {
    config: SharedConfig,
    runtime: TaskRuntime,
    desktop: Arc<AppDesktop>,
    media: Arc<MediaService>,
}

impl AppServices {
    /// 使用显式后台运行时构建服务容器。
    ///
    /// 守护进程在独立运行时上启动异步服务，同时把 winit 事件循环放在主线程；
    /// 该构造函数让任务运行时与 GUI 线程的生命周期分离。
    pub(crate) fn with_runtime(config: Config, runtime: TaskRuntime) -> Result<Self, PortError> {
        let shared = shared_config(config);
        let desktop = Arc::new(AppDesktop::new(shared.clone(), runtime.clone())?);
        let media = Arc::new(MediaService::new(shared.clone(), runtime.clone()));
        Ok(Self {
            config: shared,
            runtime,
            desktop,
            media,
        })
    }

    /// 获取当前配置快照。
    pub(crate) fn config(&self) -> Arc<Config> {
        crate::config::config_snapshot(&self.config)
    }

    /// 发布新配置快照，使后续媒体任务与桌面通知使用新设置。
    pub(crate) fn update_config(&self, config: Config) {
        publish_config(&self.config, config);
    }

    /// 获取桌面副作用端口。
    pub(crate) fn desktop(&self) -> Arc<dyn DesktopPort> {
        self.desktop.clone()
    }

    /// 获取媒体任务执行端口。
    pub(crate) fn media(&self) -> Arc<dyn MediaJobRunner> {
        self.media.clone()
    }

    /// 获取统一后台任务运行时。
    pub(crate) fn runtime(&self) -> TaskRuntime {
        self.runtime.clone()
    }
}
