//! 网易有道翻译 OpenAPI 适配器。

use crate::error::ServiceError;
use crate::translate::engine::Translator;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// 网易有道翻译服务适配器
pub struct YoudaoTranslator {
    client: Client,
    app_key: String,
    app_secret: String,
}

impl YoudaoTranslator {
    /// 构造新的网易有道翻译适配器
    pub fn new(app_key: String, app_secret: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            client,
            app_key,
            app_secret,
        }
    }

    /// 有道 v3 截断规则算法
    fn truncate_input(q: &str) -> String {
        let char_count = q.chars().count();
        if char_count <= 20 {
            q.to_string()
        } else {
            let prefix: String = q.chars().take(10).collect();
            let suffix: String = q.chars().skip(char_count - 10).collect();
            format!("{}{}{}", prefix, char_count, suffix)
        }
    }

    /// 规范化语言代号 (有道格式)
    fn normalize_lang(lang: &str) -> &str {
        match lang {
            "zh" | "zh-CN" | "zh-Hans" => "zh-CHS",
            "zh-TW" | "zh-Hant" => "zh-CHT",
            "en" => "en",
            "ja" => "ja",
            "ko" => "ko",
            "fr" => "fr",
            "es" => "es",
            "ru" => "ru",
            _ => lang,
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

        let from_lang = Self::normalize_lang(from);
        let to_lang = Self::normalize_lang(to);

        let query = texts.join("\n");
        let salt = "snapin_youdao_salt";
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

        let url = "https://openapi.youdao.com/api";
        let response = self
            .client
            .post(url)
            .form(&[
                ("q", query.as_str()),
                ("from", from_lang),
                ("to", to_lang),
                ("appKey", self.app_key.as_str()),
                ("salt", salt),
                ("sign", sign.as_str()),
                ("signType", "v3"),
                ("curtime", curtime.as_str()),
            ])
            .send()
            .await
            .map_err(|e| ServiceError::NetworkError {
                provider: "youdao".to_string(),
                source: e,
            })?;

        let data: YoudaoResponse =
            response
                .json()
                .await
                .map_err(|e| ServiceError::NetworkError {
                    provider: "youdao".to_string(),
                    source: e,
                })?;

        if data.error_code != "0" {
            return Err(ServiceError::ApiError {
                provider: "youdao".to_string(),
                code: data.error_code,
                message: "有道云翻译请求未成功".to_string(),
            });
        }

        if let Some(trans) = data.translation {
            // 有道通常返回单个合并文本，按行切割以匹配各文本行
            let lines: Vec<String> = trans
                .into_iter()
                .flat_map(|s| s.lines().map(|l| l.to_string()).collect::<Vec<_>>())
                .collect();
            if lines.len() == texts.len() {
                Ok(lines)
            } else {
                Ok(texts.to_vec())
            }
        } else {
            Ok(texts.to_vec())
        }
    }
}
