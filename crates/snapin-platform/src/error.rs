//! Snapin 平台抽象层领域错误模型。
//!
//! 封装 Portal 截屏、D-Bus 进程通信、GNOME 扩展客户端交互、剪贴板与底层 I/O 异常。

use snapin_core::error::CoreError;
use thiserror::Error;

/// 平台抽象层领域错误枚举
#[derive(Error, Debug)]
pub enum PlatformError {
    /// XDG Desktop Portal 截屏服务请求失败
    #[error("XDG Desktop Portal 截屏服务请求失败，原因: {message}")]
    Portal { message: String },

    /// D-Bus 进程间通信或扩展调用异常
    #[error("D-Bus 进程间通信异常，目标服务: {service}，原因: {message}")]
    DBus { service: String, message: String },

    /// 系统剪贴板操作失败
    #[error("系统剪贴板操作失败，描述: {message}")]
    Clipboard { message: String },

    /// 平台文件系统 I/O 异常
    #[error("平台文件系统 I/O 操作失败，路径: {path}，原因: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// 截屏结果图像解码失败。
    ///
    /// 编解码库的错误在跨界处即转换为本领域变体，避免 `image` 这类大型依赖的错误枚举
    /// 变动直接破坏平台层对外的错误契约。
    #[error("截屏图像内存解码失败，原因: {message}")]
    ImageDecode { message: String },

    /// 核心领域错误透明转换
    #[error(transparent)]
    Core(#[from] CoreError),
}
