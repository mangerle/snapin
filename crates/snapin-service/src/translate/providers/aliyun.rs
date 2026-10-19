//! 阿里云机器翻译 API 适配器。

use crate::error::ServiceError;
use crate::translate::engine::Translator;
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::Deserialize;
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;

/// 阿里云机器翻译服务适配器
pub struct AliyunTranslator {
    client: Client,
    access_key_id: String,
    access_key_secret: String,
}

impl AliyunTranslator {
    /// 构造新的阿里云翻译适配器
    pub fn new(access_key_id: String, access_key_secret: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            client,
            access_key_id,
            access_key_secret,
        }
    }

    /// 规范化语言代号 (阿里格式)
    fn normalize_lang(lang: &str) -> &str {
        match lang {
            "zh" | "zh-CN" | "zh-Hans" => "zh",
            "zh-TW" | "zh-Hant" => "zh-tw",
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
struct AliyunResponse {
    #[serde(rename = "Code")]
    code: Option<String>,
    #[serde(rename = "Message")]
    message: Option<String>,
    #[serde(rename = "Data")]
    data: Option<AliyunData>,
}

#[derive(Deserialize)]
struct AliyunData {
    #[serde(rename = "Translated")]
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

        let source_lang = Self::normalize_lang(from);
        let target_lang = Self::normalize_lang(to);

        let query = texts.join("\n");
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
            ("SourceText", query.as_str()),
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
        let mut mac = HmacSha1::new_from_slice(secret_key.as_bytes()).map_err(|e| {
            ServiceError::InferenceError {
                stage: "aliyun_signature".to_string(),
                reason: e.to_string(),
            }
        })?;
        mac.update(string_to_sign.as_bytes());
        use base64::Engine;
        let signature = base64::prelude::BASE64_STANDARD.encode(mac.finalize().into_bytes());

        params.push(("Signature", signature.as_str()));

        let url = "https://mt.aliyuncs.com/";
        let response = self
            .client
            .post(url)
            .form(&params)
            .send()
            .await
            .map_err(|e| ServiceError::NetworkError {
                provider: "aliyun".to_string(),
                source: e,
            })?;

        let res_data: AliyunResponse =
            response
                .json()
                .await
                .map_err(|e| ServiceError::NetworkError {
                    provider: "aliyun".to_string(),
                    source: e,
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

        if let Some(data) = res_data.data {
            if let Some(trans) = data.translated {
                let lines: Vec<String> = trans.lines().map(|s| s.to_string()).collect();
                if lines.len() == texts.len() {
                    return Ok(lines);
                }
            }
        }

        Ok(texts.to_vec())
    }
}
