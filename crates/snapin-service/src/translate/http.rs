//! 翻译服务共享 HTTP 客户端、状态码校验与有界响应读取。

use crate::error::ServiceError;
use reqwest::header::RETRY_AFTER;
use reqwest::{Client, Response};
use std::sync::OnceLock;
use std::time::Duration;

/// 单次翻译响应的最大字节数，防止异常服务端返回体耗尽内存
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

static SHARED_CLIENT: OnceLock<Client> = OnceLock::new();

/// 获取进程级共享 HTTP 客户端，复用连接池、Keep-Alive 与 DNS 缓存。
pub fn shared_client() -> Client {
    SHARED_CLIENT
        .get_or_init(|| {
            Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(5))
                // 必应免费接口的令牌与 Cookie 绑定同一次页面访问，必须保留服务端下发的 Cookie
                .cookie_store(true)
                .build()
                .unwrap_or_else(|error| {
                    tracing::warn!("构建共享 HTTP 客户端失败，降级为默认客户端: {error}");
                    Client::new()
                })
        })
        .clone()
}

/// 将底层 HTTP 错误转换为服务层领域错误。
///
/// 不拼接 `reqwest::Error` 的完整 `Display`：其中可能包含带 OCR 原文的查询串
/// 或必应会话令牌。这里只保留可安全展示的错误类别，详细原因由调用方在
/// 需要时结合上下文记录，但绝不把 URL 写入日志。
pub fn network_error(provider: &str, error: reqwest::Error) -> ServiceError {
    let message = if error.is_timeout() {
        "请求超时"
    } else if error.is_connect() {
        "建立连接失败"
    } else if error.is_body() {
        "读取响应体失败"
    } else if error.is_decode() {
        "解析响应失败"
    } else {
        "网络请求失败"
    };
    ServiceError::NetworkError {
        provider: provider.to_string(),
        message: message.to_string(),
    }
}

/// 校验 HTTP 状态码，并把 429 与其它非成功状态区分开。
pub fn check_status(provider: &str, response: &Response) -> Result<(), ServiceError> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after_secs = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok());
        return Err(ServiceError::RateLimited {
            provider: provider.to_string(),
            retry_after_secs,
        });
    }
    Err(ServiceError::ApiError {
        provider: provider.to_string(),
        code: status.as_u16().to_string(),
        message: format!("服务商返回 HTTP 状态码 {}", status.as_u16()),
    })
}

/// 以表单 POST 发送已签名请求，并统一完成状态码校验与有界响应读取。
///
/// 百度、有道、阿里三个签名服务商只在参数构造与响应解码上不同，传输层的
/// 超时、限流识别、状态码与响应体上限必须共用同一份实现，避免各自遗漏。
pub async fn post_signed_form(
    provider: &str,
    client: &Client,
    endpoint: &str,
    params: &[(&str, &str)],
) -> Result<Vec<u8>, ServiceError> {
    let response = client
        .post(endpoint)
        .form(params)
        .send()
        .await
        .map_err(|error| network_error(provider, error))?;
    check_status(provider, &response)?;
    read_limited_body(provider, response).await
}

/// 读取响应体并强制大小上限。
///
/// `content_length` 只是快速拒绝提示，最终仍按实际分块累计，防止分块传输
/// 或伪造长度绕过上限。
pub async fn read_limited_body(
    provider: &str,
    mut response: Response,
) -> Result<Vec<u8>, ServiceError> {
    if let Some(length) = response.content_length() {
        if length > MAX_RESPONSE_BYTES as u64 {
            return Err(ServiceError::ResponseTooLarge {
                provider: provider.to_string(),
                limit_bytes: MAX_RESPONSE_BYTES,
            });
        }
    }

    let mut body = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or(0)
            .min(MAX_RESPONSE_BYTES as u64) as usize,
    );
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| network_error(provider, error))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(ServiceError::ResponseTooLarge {
                provider: provider.to_string(),
                limit_bytes: MAX_RESPONSE_BYTES,
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
