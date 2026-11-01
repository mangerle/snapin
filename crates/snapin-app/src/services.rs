//! Snapin 应用服务容器：把配置快照、桌面端口与媒体执行器装配在一起。
//!
//! 守护进程、单次截图与单次贴图都通过该容器获取宿主能力，避免各自读取配置、
//! 创建线程或重复初始化 OCR/翻译服务。容器本身不依赖 GUI 事件循环，便于在
//! 应用层集成测试中替换端口。

use crate::config::{publish_config, shared_config, AppConfigStore, SharedConfig};
use crate::desktop::AppDesktop;
use crate::error::AppError;
use crate::media::MediaService;
use snapin_core::config::Config;
use snapin_ui::{ConfigStore, DesktopPort, MediaJobRunner};
use std::sync::Arc;

/// 应用级服务容器。
pub struct AppServices {
    config: SharedConfig,
    desktop: Arc<AppDesktop>,
    media: Arc<MediaService>,
    config_store: Arc<AppConfigStore>,
}

impl AppServices {
    /// 使用初始配置构建全部服务。
    ///
    /// # Errors
    ///
    /// 媒体执行器或通知工作线程创建失败时返回 [`AppError`]。
    pub fn new(config: Config) -> Result<Self, AppError> {
        let shared = shared_config(config);
        let desktop = Arc::new(AppDesktop::new(shared.clone())?);
        let media = Arc::new(MediaService::new(shared.clone())?);
        let config_store = Arc::new(AppConfigStore::new(shared.clone()));
        Ok(Self {
            config: shared,
            desktop,
            media,
            config_store,
        })
    }

    /// 获取当前配置快照。
    pub fn config(&self) -> Arc<Config> {
        crate::config::config_snapshot(&self.config)
    }

    /// 发布新配置快照，使后续媒体任务与桌面通知使用新设置。
    pub fn update_config(&self, config: Config) {
        publish_config(&self.config, config);
    }

    /// 获取桌面副作用端口。
    pub fn desktop(&self) -> Arc<dyn DesktopPort> {
        self.desktop.clone()
    }

    /// 获取媒体任务执行端口。
    pub fn media(&self) -> Arc<dyn MediaJobRunner> {
        self.media.clone()
    }

    /// 获取配置持久化端口。
    pub fn config_store(&self) -> Arc<dyn ConfigStore> {
        self.config_store.clone()
    }
}
