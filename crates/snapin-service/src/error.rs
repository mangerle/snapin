//! Snapin 服务层统一错误类型定义。

use thiserror::Error;

/// 服务层统一错误枚举
#[derive(Debug, Error)]
#[non_exhaustive]
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

    /// 服务商限流或配额耗尽
    #[error("服务商限流，服务商: {provider}，建议等待: {retry_after_secs:?} 秒")]
    RateLimited {
        provider: String,
        retry_after_secs: Option<u64>,
    },

    /// 响应体超过安全上限
    #[error("服务商响应体过大，服务商: {provider}，上限: {limit_bytes} 字节")]
    ResponseTooLarge {
        provider: String,
        limit_bytes: usize,
    },

    /// 响应格式不符合预期
    #[error("服务商响应格式异常，服务商: {provider}，原因: {message}")]
    ResponseFormat { provider: String, message: String },

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

impl ServiceError {
    /// 是否为可安全重试的瞬时故障。
    ///
    /// 业务错误、凭据缺失与响应格式错误重试不会改变结果；只有网络抖动、
    /// 限流与超时值得在退避后重试。
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::NetworkError { .. } | Self::RateLimited { .. })
    }
}
