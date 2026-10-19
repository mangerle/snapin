//! 免费谷歌翻译接口适配器 (基于 Google GTX 免 Key 开放通道)。

use crate::error::ServiceError;
use crate::translate::engine::Translator;
use async_trait::async_trait;
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
    /// 构造新的谷歌翻译客户端实例
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    /// 规范化语言代号 (谷歌格式)
    fn normalize_lang(lang: &str) -> &str {
        match lang {
            "zh" | "zh-CN" | "zh-Hans" => "zh-CN",
            "zh-TW" | "zh-Hant" => "zh-TW",
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
impl Translator for GoogleFreeTranslator {
    async fn translate_batch(
        &self,
        texts: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, ServiceError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let sl = Self::normalize_lang(from);
        let tl = Self::normalize_lang(to);

        let mut results = Vec::with_capacity(texts.len());

        // 谷歌 GTX 接口针对单句或分段翻译稳定性最高，并发逐句调用
        for text in texts {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                results.push(String::new());
                continue;
            }

            let url = "https://translate.googleapis.com/translate_a/single";
            let response = self
                .client
                .get(url)
                .query(&[
                    ("client", "gtx"),
                    ("sl", sl),
                    ("tl", tl),
                    ("dt", "t"),
                    ("q", trimmed),
                ])
                .send()
                .await
                .map_err(|e| ServiceError::NetworkError {
                    provider: "google_free".to_string(),
                    source: e,
                })?;

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
                .map_err(|e| ServiceError::NetworkError {
                    provider: "google_free".to_string(),
                    source: e,
                })?;

            // 谷歌响应结构: [[[ "译文", "原文", ... ], ...], ...]
            let mut translated_text = String::new();
            if let Some(sentences) = json.get(0).and_then(|v| v.as_array()) {
                for s in sentences {
                    if let Some(part) = s.get(0).and_then(|p| p.as_str()) {
                        translated_text.push_str(part);
                    }
                }
            }

            if translated_text.is_empty() {
                results.push(trimmed.to_string());
            } else {
                results.push(translated_text);
            }
        }

        Ok(results)
    }
}
