//! Snapin 核心领域基础错误模型定义。
//!
//! 采用 `thiserror` 规范化管理配置解析、序列化与核心 I/O 错误，所有错误文本附带明确的
//! 中文上下文参数，便于精准定位现场。核心层不做图像编解码，因此本枚举不引用任何
//! 重型第三方编解码错误类型，避免 `image` 的错误枚举变动破坏核心层的对外契约。

use thiserror::Error;

/// Snapin 核心领域基础错误枚举
#[derive(Error, Debug)]
pub enum CoreError {
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
}
