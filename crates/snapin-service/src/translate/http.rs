//! 翻译服务共享 HTTP 客户端与网络错误转换。

use crate::error::ServiceError;
use reqwest::Client;
use std::sync::OnceLock;
use std::time::Duration;

static SHARED_CLIENT: OnceLock<Client> = OnceLock::new();

/// 获取进程级共享 HTTP 客户端，复用连接池、Keep-Alive 与 DNS 缓存。
pub fn shared_client() -> Client {
    SHARED_CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|error| {
                    tracing::warn!("构建共享 HTTP 客户端失败，降级为默认客户端: {error}");
                    Client::new()
                })
        })
        .clone()
}

/// 将底层 HTTP 错误转换为服务层领域错误，避免泄漏第三方错误类型。
pub fn network_error(provider: &str, error: reqwest::Error) -> ServiceError {
    ServiceError::NetworkError {
        provider: provider.to_string(),
        message: error.to_string(),
    }
}
