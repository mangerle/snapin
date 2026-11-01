//! Snapin 应用层错误模型。

use thiserror::Error;

/// 应用层初始化与端口适配错误。
///
/// 该枚举只描述应用层自身的失败（线程创建、配置定位等），底层平台与服务错误
/// 在进入端口或任务结果时已转换为可展示的中文描述，避免把第三方错误类型
/// 泄漏到 UI 或命令处理层。
#[derive(Debug, Error)]
pub enum AppError {
    /// 后台媒体执行器线程创建失败
    #[error("创建媒体任务工作线程失败，原因: {reason}")]
    WorkerSpawn {
        /// 底层线程创建错误描述
        reason: String,
    },

    /// 桌面通知工作线程创建失败
    #[error("创建桌面通知工作线程失败，原因: {reason}")]
    NotificationSpawn {
        /// 底层线程创建错误描述
        reason: String,
    },
}
