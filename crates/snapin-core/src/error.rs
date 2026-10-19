//! Snapin 全局统一错误模型定义。
//!
//! 采用 `thiserror` 规范化管理所有系统调用、D-Bus 通信、图像编码与 I/O 错误，
//! 所有错误文本附带明确的中文上下文参数，便于精准定位现场。

use thiserror::Error;

/// Snapin 统一领域错误枚举
#[derive(Error, Debug)]
pub enum SnapinError {
    /// XDG Desktop Portal 截屏通信失败
    #[error("XDG Desktop Portal 截屏服务请求失败，原因: {message}")]
    PortalError { message: String },

    /// D-Bus 通信或方法调用异常
    #[error("D-Bus 进程间通信异常，目标服务: {service}，原因: {message}")]
    DBusError { service: String, message: String },

    /// Wayland 或底层操作系统窗口与事件循环异常
    #[error("Wayland 窗口系统异常，描述: {message}")]
    WindowError { message: String },

    /// 帧缓冲分配与渲染呈现异常
    #[error("softbuffer 共享内存帧缓冲操作失败，描述: {message}")]
    SoftbufferError { message: String },

    /// 图像像素裁剪、编解码失败
    #[error("图像数据处理或编解码异常，原因: {0}")]
    ImageError(#[from] image::ImageError),

    /// 本地文件读取或写入 I/O 异常
    #[error("本地文件系统 I/O 操作失败，路径: {path}，原因: {source}")]
    IoError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// TOML 配置文件解析与反序列化失败
    #[error("TOML 配置文件解析失败，路径: {path}，原因: {source}")]
    ConfigParseError {
        path: String,
        #[source]
        source: toml::de::Error,
    },

    /// TOML 配置文件序列化与生成失败
    #[error("TOML 配置文件序列化失败，原因: {0}")]
    ConfigSerializeError(#[from] toml::ser::Error),

    /// 本地 Unix Domain Socket 通信失败
    #[error("单实例 IPC 套接字通信异常，套接字路径: {socket_path}，原因: {message}")]
    IpcError {
        socket_path: String,
        message: String,
    },

    /// 剪贴板内容获取或格式转换异常
    #[error("系统剪贴板读取失败，不支持的 MIME 类型或数据损坏")]
    ClipboardEmptyOrUnsupported,

    /// 剪贴板读写操作异常
    #[error("系统剪贴板操作失败，描述: {message}")]
    ClipboardError { message: String },
}
