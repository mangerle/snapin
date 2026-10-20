//! Snapin 服务层统一错误类型定义。

use thiserror::Error;

/// 服务层统一错误枚举
#[derive(Debug, Error)]
pub enum ServiceError {
    /// 模型文件不存在或损坏
    #[error("OCR 模型文件不存在，目标路径: {path}")]
    ModelNotFound { path: String },

    /// 模型配置或字典解析失败
    #[error("解析 OCR 模型配置失败，文件路径: {path}，原因: {reason}")]
    ConfigParseError { path: String, reason: String },

    /// 深度学习推理执行失败
    #[error("OCR 推理执行异常，环节: {stage}，原因: {reason}")]
    InferenceError { stage: String, reason: String },

    /// 网络请求错误
    #[error("服务请求网络异常，服务商: {provider}，原因: {message}")]
    NetworkError { provider: String, message: String },

    /// 第三方 API 响应解析或返回业务错误
    #[error("服务商业务报错，服务商: {provider}，错误码: {code}，详情: {message}")]
    ApiError {
        provider: String,
        code: String,
        message: String,
    },

    /// 缺少必要的 API Key 或凭据配置
    #[error("未配置必要的认证凭据，服务商: {provider}，缺失字段: {field}")]
    MissingCredentials { provider: String, field: String },

    /// IO 错误
    #[error("本地文件读写失败，路径: {path}，原因: {source}")]
    IoError {
        path: String,
        source: std::io::Error,
    },
}
