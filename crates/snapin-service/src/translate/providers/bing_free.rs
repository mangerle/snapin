//! 免费微软必应/Edge 翻译适配器 (免 Key 开放接口)。

use crate::error::ServiceError;
use crate::translate::config::TranslateProvider;
use crate::translate::engine::Translator;
use crate::translate::http::{network_error, shared_client};
use crate::translate::language::normalize_language;
use async_trait::async_trait;
use futures::stream::{self, StreamExt};
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
    /// 构造新的必应翻译适配器实例（复用进程级 HTTP 连接池）
    pub fn new() -> Self {
        Self {
            client: shared_client(),
        }
    }

    async fn translate_one(
        client: &Client,
        text: &str,
        from_lang: Option<&str>,
        to_lang: &str,
    ) -> Result<String, ServiceError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(String::new());
        }

        let url = "https://www.bing.com/ttranslatev3";
        let mut params = vec![("to", to_lang), ("text", trimmed)];
        if let Some(from_lang) = from_lang {
            params.push(("fromLang", from_lang));
        }

        let response = client
            .post(url)
            .form(&params)
            .send()
            .await
            .map_err(|error| network_error("bing_free", error))?;

        if !response.status().is_success() {
            return Err(ServiceError::ApiError {
                provider: "bing_free".to_string(),
                code: response.status().to_string(),
                message: "请求必应翻译接口被拦截或限流".to_string(),
            });
        }

        let json: Value = response
            .json()
            .await
            .map_err(|error| network_error("bing_free", error))?;

        let mut translated = String::new();
        if let Some(item) = json.as_array().and_then(|array| array.first()) {
            if let Some(translations) = item.get("translations").and_then(|value| value.as_array())
            {
                if let Some(first) = translations.first() {
                    if let Some(text) = first.get("text").and_then(|value| value.as_str()) {
                        translated = text.to_string();
                    }
                }
            }
        }

        if translated.is_empty() {
            return Err(ServiceError::ApiError {
                provider: "bing_free".to_string(),
                code: "EMPTY_RESULT".to_string(),
                message: "必应翻译接口未返回译文".to_string(),
            });
        }

        Ok(translated)
    }
}

#[async_trait]
impl Translator for BingFreeTranslator {
    fn name(&self) -> &'static str {
        "bing_free"
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

        let from_lang = if from == "auto" {
            None
        } else {
            Some(normalize_language(TranslateProvider::BingFree, from).to_string())
        };
        let to_lang = normalize_language(TranslateProvider::BingFree, to).to_string();

        let results = stream::iter(texts.iter().cloned().enumerate())
            .map(|(index, text)| {
                let client = self.client.clone();
                let from_lang = from_lang.clone();
                let to_lang = to_lang.clone();
                async move {
                    let result =
                        Self::translate_one(&client, &text, from_lang.as_deref(), &to_lang).await;
                    (index, result)
                }
            })
            .buffer_unordered(4)
            .collect::<Vec<_>>()
            .await;

        let mut ordered = vec![String::new(); texts.len()];
        for (index, result) in results {
            if let Some(slot) = ordered.get_mut(index) {
                *slot = result?;
            }
        }
        Ok(ordered)
    }
}
