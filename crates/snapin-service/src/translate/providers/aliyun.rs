//! 阿里云机器翻译 API 适配器。

use crate::error::ServiceError;
use crate::translate::batch::{join_batch, merge_joined_results, plan_batches, BatchLimits};
use crate::translate::config::TranslateProvider;
use crate::translate::engine::Translator;
use crate::translate::http::{check_status, network_error, read_limited_body, shared_client};
use crate::translate::language::normalize_language;
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::Deserialize;
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;

/// 阿里云机器翻译开放接口地址
const ALIYUN_ENDPOINT: &str = "https://mt.aliyuncs.com/";

/// 阿里云单次请求的文本长度与条数预算
const ALIYUN_BATCH_LIMITS: BatchLimits = BatchLimits {
    max_items: 20,
    max_total_bytes: 4_500,
    max_text_bytes: 2_000,
};

/// 阿里云机器翻译服务适配器
pub struct AliyunTranslator {
    client: Client,
    endpoint: String,
    access_key_id: String,
    access_key_secret: String,
}

impl AliyunTranslator {
    /// 构造新的阿里云翻译适配器
    pub fn new(access_key_id: String, access_key_secret: String) -> Self {
        Self::with_endpoint(ALIYUN_ENDPOINT, access_key_id, access_key_secret)
    }

    /// 以指定端点构造适配器，供集成测试注入本地 Mock 服务校验响应契约。
    pub(crate) fn with_endpoint(
        endpoint: impl Into<String>,
        access_key_id: String,
        access_key_secret: String,
    ) -> Self {
        Self {
            client: shared_client(),
            endpoint: endpoint.into(),
            access_key_id,
            access_key_secret,
        }
    }

    /// 发送一次合并翻译请求，并校验业务错误码与返回条数。
    async fn request_joined(
        &self,
        query: &str,
        source_lang: &str,
        target_lang: &str,
        expected: usize,
    ) -> Result<Vec<String>, ServiceError> {
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();

        let mut params = vec![
            ("AccessKeyId", self.access_key_id.as_str()),
            ("Action", "TranslateGeneral"),
            ("Format", "JSON"),
            ("FormatType", "text"),
            ("Scene", "general"),
            ("SignatureMethod", "HMAC-SHA1"),
            ("SignatureNonce", nonce.as_str()),
            ("SignatureVersion", "1.0"),
            ("SourceLanguage", source_lang),
            ("SourceText", query),
            ("TargetLanguage", target_lang),
            ("Timestamp", timestamp.as_str()),
            ("Version", "2018-10-12"),
        ];

        params.sort_by(|a, b| a.0.cmp(b.0));

        let canonicalized_query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let string_to_sign = format!("POST&%2F&{}", percent_encode(&canonicalized_query_string));

        let secret_key = format!("{}&", self.access_key_secret);
        let mut mac = HmacSha1::new_from_slice(secret_key.as_bytes()).map_err(|error| {
            ServiceError::ApiError {
                provider: "aliyun".to_string(),
                code: "SIGNATURE_INIT".to_string(),
                message: format!("初始化阿里云 HMAC-SHA1 签名器失败: {error}"),
            }
        })?;
        mac.update(string_to_sign.as_bytes());
        use base64::Engine;
        let signature = base64::prelude::BASE64_STANDARD.encode(mac.finalize().into_bytes());

        params.push(("Signature", signature.as_str()));

        let response = self
            .client
            .post(self.endpoint.as_str())
            .form(&params)
            .send()
            .await
            .map_err(|error| network_error("aliyun", error))?;
        check_status("aliyun", &response)?;

        let body = read_limited_body("aliyun", response).await?;
        let res_data: AliyunResponse =
            serde_json::from_slice(&body).map_err(|error| ServiceError::ResponseFormat {
                provider: "aliyun".to_string(),
                message: format!("无法解析阿里云翻译 JSON 响应: {error}"),
            })?;

        if let Some(code) = res_data.code {
            if code != "200" {
                return Err(ServiceError::ApiError {
                    provider: "aliyun".to_string(),
                    code,
                    message: res_data.message.unwrap_or_default(),
                });
            }
        }

        let translated = res_data
            .data
            .and_then(|data| data.translated)
            .ok_or_else(|| ServiceError::ApiError {
                provider: "aliyun".to_string(),
                code: "EMPTY_RESULT".to_string(),
                message: "阿里云翻译接口未返回译文".to_string(),
            })?;

        let lines: Vec<String> = translated.lines().map(str::to_string).collect();
        if lines.len() != expected {
            return Err(ServiceError::ApiError {
                provider: "aliyun".to_string(),
                code: "RESULT_COUNT_MISMATCH".to_string(),
                message: format!(
                    "阿里云翻译返回 {} 条译文，与请求 {expected} 条不一致",
                    lines.len()
                ),
            });
        }
        Ok(lines)
    }
}

#[derive(Deserialize)]
struct AliyunResponse {
    #[serde(rename = "Code", default)]
    code: Option<String>,
    #[serde(rename = "Message", default)]
    message: Option<String>,
    #[serde(rename = "Data", default)]
    data: Option<AliyunData>,
}

#[derive(Deserialize)]
struct AliyunData {
    #[serde(rename = "Translated", default)]
    translated: Option<String>,
}

/// 阿里云规范要求的 RFC 3986 百分号编码
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{:02X}", b);
        }
    }
    out
}

#[async_trait]
impl Translator for AliyunTranslator {
    fn name(&self) -> &'static str {
        "aliyun"
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

        if self.access_key_id.trim().is_empty() || self.access_key_secret.trim().is_empty() {
            return Err(ServiceError::MissingCredentials {
                provider: "aliyun".to_string(),
                field: "aliyun_access_key_id 或 aliyun_access_key_secret".to_string(),
            });
        }

        let source_lang = normalize_language(TranslateProvider::Aliyun, from);
        let target_lang = normalize_language(TranslateProvider::Aliyun, to);
        let mut results = Vec::with_capacity(texts.len());

        for batch in plan_batches(texts, ALIYUN_BATCH_LIMITS) {
            let (query, indices) = join_batch(batch);
            if query.is_empty() {
                results.extend(std::iter::repeat_n(String::new(), batch.len()));
                continue;
            }
            let translated = self
                .request_joined(&query, source_lang, target_lang, indices.len())
                .await?;
            results.extend(merge_joined_results(batch.len(), &indices, translated)?);
        }

        Ok(results)
    }
}
