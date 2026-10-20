//! 免费谷歌翻译接口适配器 (基于 Google GTX 免 Key 开放通道)。

use crate::error::ServiceError;
use crate::translate::config::TranslateProvider;
use crate::translate::engine::Translator;
use crate::translate::http::{network_error, shared_client};
use crate::translate::language::normalize_language;
use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde_json::Value;

/// 免费谷歌翻译服务适配器
pub struct GoogleFreeTranslator {
    client: Client,
}

impl Default for GoogleFreeTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleFreeTranslator {
    /// 构造新的谷歌翻译客户端实例（复用进程级 HTTP 连接池）
    pub fn new() -> Self {
        Self {
            client: shared_client(),
        }
    }

    async fn translate_one(
        client: &Client,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<String, ServiceError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(String::new());
        }

        let url = "https://translate.googleapis.com/translate_a/single";
        let response = client
            .get(url)
            .query(&[
                ("client", "gtx"),
                ("sl", source_lang),
                ("tl", target_lang),
                ("dt", "t"),
                ("q", trimmed),
            ])
            .send()
            .await
            .map_err(|error| network_error("google_free", error))?;

        if !response.status().is_success() {
            return Err(ServiceError::ApiError {
                provider: "google_free".to_string(),
                code: response.status().to_string(),
                message: "请求谷歌翻译接口被拦截或限流".to_string(),
            });
        }

        let json: Value = response
            .json()
            .await
            .map_err(|error| network_error("google_free", error))?;

        let mut translated = String::new();
        if let Some(sentences) = json.get(0).and_then(|value| value.as_array()) {
            for sentence in sentences {
                if let Some(part) = sentence.get(0).and_then(|value| value.as_str()) {
                    translated.push_str(part);
                }
            }
        }

        if translated.is_empty() {
            return Err(ServiceError::ApiError {
                provider: "google_free".to_string(),
                code: "EMPTY_RESULT".to_string(),
                message: "谷歌翻译接口未返回译文".to_string(),
            });
        }

        Ok(translated)
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

        let results = stream::iter(texts.iter().cloned().enumerate())
            .map(|(index, text)| {
                let client = self.client.clone();
                let source_lang = source_lang.clone();
                let target_lang = target_lang.clone();
                async move {
                    let result =
                        Self::translate_one(&client, &text, &source_lang, &target_lang).await;
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
