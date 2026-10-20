//! Snapin 核心领域基础错误模型定义。
//!
//! 采用 `thiserror` 规范化管理配置解析、序列化、基础图像编解码与核心 I/O 错误，
//! 所有错误文本附带明确的中文上下文参数，便于精准定位现场。

use thiserror::Error;

/// Snapin 核心领域基础错误枚举
#[derive(Error, Debug)]
pub enum CoreError {
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
}
