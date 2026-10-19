//! 免费微软必应/Edge 翻译适配器 (免 Key 开放接口)。

use crate::error::ServiceError;
use crate::translate::engine::Translator;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

/// 免费微软必应翻译服务适配器
pub struct BingFreeTranslator {
    client: Client,
}

impl Default for BingFreeTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl BingFreeTranslator {
    /// 构造新的必应翻译适配器实例
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    /// 规范化语言代号 (微软格式)
    fn normalize_lang(lang: &str) -> &str {
        match lang {
            "zh" | "zh-CN" | "zh-Hans" => "zh-Hans",
            "zh-TW" | "zh-Hant" => "zh-Hant",
            "en" => "en",
            "ja" => "ja",
            "ko" => "ko",
            "fr" => "fr",
            "de" => "de",
            "es" => "es",
            "ru" => "ru",
            _ => lang,
        }
    }
}

#[async_trait]
impl Translator for BingFreeTranslator {
    async fn translate_batch(
        &self,
        texts: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, ServiceError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let to_lang = Self::normalize_lang(to);
        let from_lang = if from == "auto" {
            ""
        } else {
            Self::normalize_lang(from)
        };

        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                results.push(String::new());
                continue;
            }

            // 采用微软 Bing 翻译轻量端点
            let url = "https://www.bing.com/ttranslatev3";
            let mut params = vec![("to", to_lang), ("text", trimmed)];
            if !from_lang.is_empty() {
                params.push(("fromLang", from_lang));
            }

            let response = self
                .client
                .post(url)
                .form(&params)
                .send()
                .await
                .map_err(|e| ServiceError::NetworkError {
                    provider: "bing_free".to_string(),
                    source: e,
                })?;

            if !response.status().is_success() {
                // 若被反爬策略拦截，降级采用保底返回
                results.push(trimmed.to_string());
                continue;
            }

            let json: Value = response
                .json()
                .await
                .map_err(|e| ServiceError::NetworkError {
                    provider: "bing_free".to_string(),
                    source: e,
                })?;

            let mut translated = String::new();
            if let Some(arr) = json.as_array().and_then(|a| a.first()) {
                if let Some(trans_arr) = arr.get("translations").and_then(|t| t.as_array()) {
                    if let Some(first) = trans_arr.first() {
                        if let Some(t_str) = first.get("text").and_then(|t| t.as_str()) {
                            translated = t_str.to_string();
                        }
                    }
                }
            }

            if translated.is_empty() {
                results.push(trimmed.to_string());
            } else {
                results.push(translated);
            }
        }

        Ok(results)
    }
}
