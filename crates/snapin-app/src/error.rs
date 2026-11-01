//! Snapin 应用层错误模型。

use thiserror::Error;

/// 应用层初始化与端口适配错误。
///
/// 该枚举只描述应用层自身的失败（统一后台运行时不可用等），底层平台与服务错误
/// 在进入端口或任务结果时已转换为可展示的中文描述，避免把第三方错误类型
/// 泄漏到 UI 或命令处理层。
#[derive(Debug, Error)]
pub enum AppError {
    /// 统一后台任务运行时不可用
    #[error("初始化统一后台任务运行时失败，原因: {reason}")]
    RuntimeUnavailable {
        /// 运行时构建或获取失败描述
        reason: String,
    },
}
