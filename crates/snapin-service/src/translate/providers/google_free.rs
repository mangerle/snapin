//! 免费谷歌翻译适配器（免 Key 开放接口）。
//!
//! 端点为 `translate.googleapis.com/translate_a/t`（`client=dict-chrome-ex`）：
//! 与 `clients5.google.com` 共用同一翻译后端，但两者在不同出口网络下的可达性
//! 会彼此反转，因此按顺序尝试并用后者兜底。
//! `translate_a/single?client=gtx` 会被谷歌按出口 IP 直接限流（429），禁止使用。
//! 接口支持一次请求携带多个 `q` 参数完成批量翻译，无需逐条并发发起请求。

use crate::error::ServiceError;
use crate::translate::config::TranslateProvider;
use crate::translate::engine::Translator;
use crate::translate::http::{network_error, shared_client};
use crate::translate::language::normalize_language;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

/// 免费谷歌翻译端点，按顺序尝试，前一个不可达时自动切换下一个
const GOOGLE_ENDPOINTS: &[&str] = &[
    "https://translate.googleapis.com/translate_a/t",
    "https://clients5.google.com/translate_a/t",
];

/// 接口要求的客户端标识，缺失或更换会导致参数校验失败
const GOOGLE_CLIENT: &str = "dict-chrome-ex";

/// 免费谷歌翻译服务适配器
pub struct GoogleFreeTranslator {
    client: Client,
    endpoints: Vec<String>,
}

impl Default for GoogleFreeTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleFreeTranslator {
    /// 构造新的谷歌翻译客户端实例（复用进程级 HTTP 连接池）
    pub fn new() -> Self {
        Self::with_endpoints(
            GOOGLE_ENDPOINTS
                .iter()
                .map(|endpoint| (*endpoint).to_string())
                .collect(),
        )
    }

    /// 以指定端点构造适配器，供集成测试注入本地 Mock 服务校验响应契约。
    #[cfg(test)]
    pub(crate) fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self::with_endpoints(vec![endpoint.into()])
    }

    pub(crate) fn with_endpoints(endpoints: Vec<String>) -> Self {
        Self {
            client: shared_client(),
            endpoints,
        }
    }

    /// 向单个端点发起批量翻译请求，并校验返回条数与内容。
    async fn request_endpoint(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
        expected: usize,
    ) -> Result<Vec<String>, ServiceError> {
        let response = self
            .client
            .get(endpoint)
            .query(params)
            .send()
            .await
            .map_err(|error| network_error("google_free", error))?;

        if !response.status().is_success() {
            return Err(ServiceError::ApiError {
                provider: "google_free".to_string(),
                code: response.status().to_string(),
                message: "请求谷歌免费翻译接口失败，可能被出口网络策略拦截".to_string(),
            });
        }

        let payload: Value = response
            .json()
            .await
            .map_err(|error| network_error("google_free", error))?;

        let items = payload.as_array().ok_or_else(|| ServiceError::ApiError {
            provider: "google_free".to_string(),
            code: "EMPTY_RESULT".to_string(),
            message: "谷歌翻译接口未返回译文列表".to_string(),
        })?;

        let results: Vec<String> = items.iter().map(extract_translation).collect();
        if results.len() != expected {
            return Err(ServiceError::ApiError {
                provider: "google_free".to_string(),
                code: "RESULT_COUNT_MISMATCH".to_string(),
                message: format!(
                    "谷歌翻译返回 {} 条译文，与请求 {expected} 条不一致",
                    results.len()
                ),
            });
        }
        if results.iter().all(|text| text.is_empty()) {
            return Err(ServiceError::ApiError {
                provider: "google_free".to_string(),
                code: "EMPTY_RESULT".to_string(),
                message: "谷歌翻译接口未返回译文".to_string(),
            });
        }

        Ok(results)
    }
}

#[async_trait]
impl Translator for GoogleFreeTranslator {
    fn name(&self) -> &'static str {
        "google_free"
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, ServiceError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let source_lang = normalize_language(TranslateProvider::GoogleFree, from).to_string();
        let target_lang = normalize_language(TranslateProvider::GoogleFree, to).to_string();

        let mut params: Vec<(&str, &str)> = vec![
            ("client", GOOGLE_CLIENT),
            ("sl", source_lang.as_str()),
            ("tl", target_lang.as_str()),
        ];
        for text in texts {
            params.push(("q", text.as_str()));
        }

        let mut first_error: Option<ServiceError> = None;
        for endpoint in &self.endpoints {
            match self.request_endpoint(endpoint, &params, texts.len()).await {
                Ok(results) => return Ok(results),
                Err(error) => {
                    if first_error.is_some() {
                        tracing::warn!("谷歌翻译备用端点同样失败，端点: {endpoint}，原因: {error}");
                    } else {
                        tracing::debug!(
                            "谷歌翻译端点请求失败，尝试备用端点: {endpoint}，原因: {error}"
                        );
                    }
                    first_error.get_or_insert(error);
                }
            }
        }

        // 端点列表由构造函数保证非空；此兜底仅防御未来新增的空配置构造路径
        Err(first_error.unwrap_or_else(|| ServiceError::ApiError {
            provider: "google_free".to_string(),
            code: "NO_ENDPOINT".to_string(),
            message: "谷歌免费翻译没有配置任何可用端点".to_string(),
        }))
    }
}

/// 提取单条译文。
///
/// 指定源语言时元素为字符串；`sl=auto` 自动检测时元素为 `[译文, 检测到的语言]` 数组，
/// 两种形态都要兼容，否则自动检测模式会被判成空结果。
fn extract_translation(item: &Value) -> String {
    match item {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .first()
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}
