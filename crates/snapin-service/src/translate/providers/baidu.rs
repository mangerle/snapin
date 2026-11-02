//! 百度通用文本翻译 API 适配器。

use crate::error::ServiceError;
use crate::translate::batch::{translate_joined_batches, BatchLimits};
use crate::translate::config::TranslateProvider;
use crate::translate::engine::Translator;
use crate::translate::http::{post_signed_form, shared_client};
use crate::translate::language::normalize_language;
use async_trait::async_trait;
use md5::{Digest, Md5};
use reqwest::Client;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// 百度通用文本翻译开放接口地址
const BAIDU_ENDPOINT: &str = "https://fanyi-api.baidu.com/api/trans/vip/translate";

/// 百度单次请求的文本长度与条数预算（接口上限约 6000 字节）
const BAIDU_BATCH_LIMITS: BatchLimits = {
    BatchLimits {
        max_items: 20,
        max_total_bytes: 5_000,
        max_text_bytes: 2_500,
    }
};

/// 百度翻译服务适配器
pub struct BaiduTranslator {
    client: Client,
    endpoint: String,
    app_id: String,
    secret_key: String,
}

impl BaiduTranslator {
    /// 构造新的百度翻译服务适配器
    pub fn new(app_id: String, secret_key: String) -> Self {
        Self::with_endpoint(BAIDU_ENDPOINT, app_id, secret_key)
    }

    /// 以指定端点构造适配器，供集成测试注入本地 Mock 服务校验响应契约。
    pub(crate) fn with_endpoint(
        endpoint: impl Into<String>,
        app_id: String,
        secret_key: String,
    ) -> Self {
        Self {
            client: shared_client(),
            endpoint: endpoint.into(),
            app_id,
            secret_key,
        }
    }

    /// 发送一次合并翻译请求，并校验业务错误码与返回条数。
    async fn request_joined(
        &self,
        query: &str,
        from_lang: &str,
        to_lang: &str,
        expected: usize,
    ) -> Result<Vec<String>, ServiceError> {
        let signature = BaiduSignature::build(query, &self.app_id, &self.secret_key);
        let body = post_signed_form(
            "baidu",
            &self.client,
            self.endpoint.as_str(),
            &[
                ("q", query),
                ("from", from_lang),
                ("to", to_lang),
                ("appid", self.app_id.as_str()),
                ("salt", signature.salt.as_str()),
                ("sign", signature.sign.as_str()),
            ],
        )
        .await?;
        parse_response(&body, expected)
    }
}

/// 百度翻译请求的盐值与 MD5 签名。
struct BaiduSignature {
    salt: String,
    sign: String,
}

impl BaiduSignature {
    /// 使用纳秒盐值计算百度要求的 MD5 签名。
    fn build(query: &str, app_id: &str, secret_key: &str) -> Self {
        let salt = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let raw_sign = format!("{app_id}{query}{salt}{secret_key}");
        let mut hasher = Md5::new();
        hasher.update(raw_sign.as_bytes());
        Self {
            salt,
            sign: format!("{:x}", hasher.finalize()),
        }
    }
}

/// 解析百度响应并校验业务码、空结果和返回条数。
fn parse_response(body: &[u8], expected: usize) -> Result<Vec<String>, ServiceError> {
    let response: BaiduResponse =
        serde_json::from_slice(body).map_err(|error| ServiceError::ResponseFormat {
            provider: "baidu".to_string(),
            message: format!("无法解析百度翻译 JSON 响应: {error}"),
        })?;
    if let Some(error_code) = response.error_code {
        if error_code != "52000" {
            return Err(ServiceError::ApiError {
                provider: "baidu".to_string(),
                code: error_code,
                message: response.error_msg.unwrap_or_default(),
            });
        }
    }
    let translated = response
        .trans_result
        .ok_or_else(|| ServiceError::ApiError {
            provider: "baidu".to_string(),
            code: "EMPTY_RESULT".to_string(),
            message: "百度翻译接口未返回译文列表".to_string(),
        })?;
    let results: Vec<String> = translated.into_iter().map(|item| item.dst).collect();
    if results.len() != expected {
        return Err(ServiceError::ApiError {
            provider: "baidu".to_string(),
            code: "RESULT_COUNT_MISMATCH".to_string(),
            message: format!(
                "百度翻译返回 {} 条译文，与请求 {expected} 条不一致",
                results.len()
            ),
        });
    }
    Ok(results)
}

#[derive(Deserialize)]
struct BaiduResponse {
    error_code: Option<String>,
    error_msg: Option<String>,
    trans_result: Option<Vec<BaiduTransItem>>,
}

#[derive(Deserialize)]
struct BaiduTransItem {
    #[serde(default)]
    dst: String,
}

#[async_trait]
impl Translator for BaiduTranslator {
    fn name(&self) -> &'static str {
        "baidu"
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

        if self.app_id.trim().is_empty() || self.secret_key.trim().is_empty() {
            return Err(ServiceError::MissingCredentials {
                provider: "baidu".to_string(),
                field: "baidu_app_id 或 baidu_secret_key".to_string(),
            });
        }

        let from_lang = normalize_language(TranslateProvider::Baidu, from);
        let to_lang = normalize_language(TranslateProvider::Baidu, to);
        translate_joined_batches(texts, BAIDU_BATCH_LIMITS, |query, expected| async move {
            self.request_joined(&query, from_lang, to_lang, expected)
                .await
        })
        .await
    }
}
