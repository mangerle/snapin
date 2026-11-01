//! 常规基础设置模型定义。

use crate::config::enums::LogLevel;
use serde::{Deserialize, Serialize};

/// 常规设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneralConfig {
    /// 是否开机自动启动
    #[serde(default = "default_true")]
    pub autostart: bool,

    /// 日志记录级别
    #[serde(default)]
    pub log_level: LogLevel,

    /// 守护进程启动成功时是否发送桌面通知
    #[serde(default)]
    pub notify_on_startup: bool,

    /// 是否在守护进程启动与周期轮询时自动检查新版本（仅提醒，不下载）
    #[serde(default = "default_true")]
    pub auto_check_update: bool,

    /// 自动检查更新的间隔小时数（默认 24 小时，最小为 1）
    #[serde(default = "default_check_interval")]
    pub update_check_interval_hours: u32,

    /// 自定义更新端点清单 URL（为空时使用官方 GitHub Releases 端点）
    #[serde(default)]
    pub custom_update_url: Option<String>,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            autostart: true,
            log_level: LogLevel::default(),
            notify_on_startup: false,
            auto_check_update: true,
            update_check_interval_hours: 24,
            custom_update_url: None,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_check_interval() -> u32 {
    24
}
