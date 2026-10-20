//! 网易有道翻译 OpenAPI 适配器。

use crate::error::ServiceError;
use crate::translate::config::TranslateProvider;
use crate::translate::engine::Translator;
use crate::translate::http::{network_error, shared_client};
use crate::translate::language::normalize_language;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// 网易有道翻译开放接口地址
const YOUDAO_ENDPOINT: &str = "https://openapi.youdao.com/api";

/// 网易有道翻译服务适配器
pub struct YoudaoTranslator {
    client: Client,
    endpoint: String,
    app_key: String,
    app_secret: String,
}

impl YoudaoTranslator {
    /// 构造新的网易有道翻译适配器
    pub fn new(app_key: String, app_secret: String) -> Self {
        Self::with_endpoint(YOUDAO_ENDPOINT, app_key, app_secret)
    }

    /// 以指定端点构造适配器，供集成测试注入本地 Mock 服务校验响应契约。
    pub(crate) fn with_endpoint(
        endpoint: impl Into<String>,
        app_key: String,
        app_secret: String,
    ) -> Self {
        Self {
            client: shared_client(),
            endpoint: endpoint.into(),
            app_key,
            app_secret,
        }
    }

    /// 有道 v3 截断规则算法
    fn truncate_input(query: &str) -> String {
        let char_count = query.chars().count();
        if char_count <= 20 {
            query.to_string()
        } else {
            let prefix: String = query.chars().take(10).collect();
            let suffix: String = query.chars().skip(char_count - 10).collect();
            format!("{}{}{}", prefix, char_count, suffix)
        }
    }
}

#[derive(Deserialize)]
struct YoudaoResponse {
    #[serde(rename = "errorCode")]
    error_code: String,
    translation: Option<Vec<String>>,
}

#[async_trait]
impl Translator for YoudaoTranslator {
    fn name(&self) -> &'static str {
        "youdao"
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

        if self.app_key.trim().is_empty() || self.app_secret.trim().is_empty() {
            return Err(ServiceError::MissingCredentials {
                provider: "youdao".to_string(),
                field: "youdao_app_key 或 youdao_app_secret".to_string(),
            });
        }

        let from_lang = normalize_language(TranslateProvider::Youdao, from);
        let to_lang = normalize_language(TranslateProvider::Youdao, to);

        let query = texts.join("\n");
        let salt = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let curtime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();

        let truncated = Self::truncate_input(&query);
        let raw_sign = format!(
            "{}{}{}{}{}",
            self.app_key, truncated, salt, curtime, self.app_secret
        );

        let mut hasher = Sha256::new();
        hasher.update(raw_sign.as_bytes());
        let sign = format!("{:x}", hasher.finalize());

        let url = self.endpoint.as_str();
        let response = self
            .client
            .post(url)
            .form(&[
                ("q", query.as_str()),
                ("from", from_lang),
                ("to", to_lang),
                ("appKey", self.app_key.as_str()),
                ("salt", salt.as_str()),
                ("sign", sign.as_str()),
                ("signType", "v3"),
                ("curtime", curtime.as_str()),
            ])
            .send()
            .await
            .map_err(|error| network_error("youdao", error))?;

        let data: YoudaoResponse = response
            .json()
            .await
            .map_err(|error| network_error("youdao", error))?;

        if data.error_code != "0" {
            return Err(ServiceError::ApiError {
                provider: "youdao".to_string(),
                code: data.error_code,
                message: "有道翻译请求未成功".to_string(),
            });
        }

        let translated = data.translation.ok_or_else(|| ServiceError::ApiError {
            provider: "youdao".to_string(),
            code: "EMPTY_RESULT".to_string(),
            message: "有道翻译接口未返回译文".to_string(),
        })?;

        // 有道通常返回单个合并文本，按行切割以匹配各文本行
        let lines: Vec<String> = translated
            .into_iter()
            .flat_map(|text| {
                text.lines()
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        if lines.len() != texts.len() {
            return Err(ServiceError::ApiError {
                provider: "youdao".to_string(),
                code: "RESULT_COUNT_MISMATCH".to_string(),
                message: format!(
                    "有道翻译返回 {} 条译文，与请求 {} 条不一致",
                    lines.len(),
                    texts.len()
                ),
            });
        }
        Ok(lines)
    }
}
