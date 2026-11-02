//! 免费谷歌翻译适配器（免 Key 开放接口）。
//!
//! 端点为 `translate.googleapis.com/translate_a/t`（`client=dict-chrome-ex`）：
//! 与 `clients5.google.com` 共用同一翻译后端，但两者在不同出口网络下的可达性
//! 会彼此反转，因此按顺序尝试并用后者兜底。
//! `translate_a/single?client=gtx` 会被谷歌按出口 IP 直接限流（429），禁止使用。
//! 接口支持一次请求携带多个 `q` 参数完成批量翻译，无需逐条并发发起请求。

use crate::error::ServiceError;
use crate::translate::batch::{plan_batches, BatchLimits};
use crate::translate::config::TranslateProvider;
use crate::translate::engine::Translator;
use crate::translate::http::{check_status, network_error, read_limited_body, shared_client};
use crate::translate::language::normalize_language;
use crate::translate::{BatchTextRequest, EndpointBatchRequest};
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

    /// 向单个端点发起一批文本的翻译请求，并校验返回条数与内容。
    async fn request_endpoint(
        &self,
        request: EndpointBatchRequest<'_>,
    ) -> Result<Vec<String>, ServiceError> {
        let batch = request.batch;
        let mut params: Vec<(&str, &str)> = Vec::with_capacity(batch.texts.len() + 3);
        params.push(("client", GOOGLE_CLIENT));
        params.push(("sl", batch.source_lang));
        params.push(("tl", batch.target_lang));
        for text in batch.texts {
            params.push(("q", text.as_str()));
        }

        let response = self
            .client
            .get(request.endpoint)
            .query(&params)
            .send()
            .await
            .map_err(|error| network_error("google_free", error))?;
        check_status("google_free", &response)?;

        let body = read_limited_body("google_free", response).await?;
        parse_response(&body, batch.texts.len())
    }

    /// 对一批文本按端点顺序尝试，全部端点失败时返回首个错误。
    async fn request_batch(
        &self,
        batch: BatchTextRequest<'_>,
    ) -> Result<Vec<String>, ServiceError> {
        let mut first_error: Option<ServiceError> = None;
        for endpoint in &self.endpoints {
            match self
                .request_endpoint(EndpointBatchRequest {
                    endpoint,
                    batch: BatchTextRequest {
                        texts: batch.texts,
                        source_lang: batch.source_lang,
                        target_lang: batch.target_lang,
                    },
                })
                .await
            {
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

        Err(first_error.unwrap_or_else(|| ServiceError::ApiError {
            provider: "google_free".to_string(),
            code: "NO_ENDPOINT".to_string(),
            message: "谷歌免费翻译没有配置任何可用端点".to_string(),
        }))
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

        // 免费接口把全部文本放进 URL 查询串；按字节预算分批，避免长文档触发 414。
        let limits = BatchLimits::new(20, 2_000, 1_500);
        let mut results = Vec::with_capacity(texts.len());
        for batch in plan_batches(texts, limits) {
            results.extend(
                self.request_batch(BatchTextRequest {
                    texts: batch,
                    source_lang: &source_lang,
                    target_lang: &target_lang,
                })
                .await?,
            );
        }
        Ok(results)
    }
}

/// 解析谷歌翻译响应并校验返回条数与空结果。
fn parse_response(body: &[u8], expected: usize) -> Result<Vec<String>, ServiceError> {
    let payload: Value =
        serde_json::from_slice(body).map_err(|error| ServiceError::ResponseFormat {
            provider: "google_free".to_string(),
            message: format!("无法解析谷歌翻译 JSON 响应: {error}"),
        })?;
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
