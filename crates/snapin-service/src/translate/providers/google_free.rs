//! 免费谷歌翻译适配器（免 Key 开放接口）。
//!
//! 端点为 `clients5.google.com/translate_a/t`（`client=dict-chrome-ex`）：
//! 同为谷歌翻译后端，但对宿主机出口 IP 不返回拦截页，而 `translate.googleapis.com`
//! 在国内网络下通常会被直接拒绝，故不采用后者。
//! 接口支持一次请求携带多个 `q` 参数完成批量翻译，无需逐条并发发起请求。

use crate::error::ServiceError;
use crate::translate::config::TranslateProvider;
use crate::translate::engine::Translator;
use crate::translate::http::{network_error, shared_client};
use crate::translate::language::normalize_language;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

/// 免费谷歌翻译接口地址
const GOOGLE_ENDPOINT: &str = "https://clients5.google.com/translate_a/t";

/// 接口要求的客户端标识，缺失或更换会导致参数校验失败
const GOOGLE_CLIENT: &str = "dict-chrome-ex";

/// 免费谷歌翻译服务适配器
pub struct GoogleFreeTranslator {
    client: Client,
    endpoint: String,
}

impl Default for GoogleFreeTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleFreeTranslator {
    /// 构造新的谷歌翻译客户端实例（复用进程级 HTTP 连接池）
    pub fn new() -> Self {
        Self::with_endpoint(GOOGLE_ENDPOINT)
    }

    /// 以指定端点构造适配器，供集成测试注入本地 Mock 服务校验响应契约。
    pub(crate) fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self {
            client: shared_client(),
            endpoint: endpoint.into(),
        }
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

        let response = self
            .client
            .get(self.endpoint.as_str())
            .query(&params)
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
        if results.len() != texts.len() {
            return Err(ServiceError::ApiError {
                provider: "google_free".to_string(),
                code: "RESULT_COUNT_MISMATCH".to_string(),
                message: format!(
                    "谷歌翻译返回 {} 条译文，与请求 {} 条不一致",
                    results.len(),
                    texts.len()
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
