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
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            autostart: true,
            log_level: LogLevel::default(),
            notify_on_startup: false,
        }
    }
}

fn default_true() -> bool {
    true
}
