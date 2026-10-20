//! 百度通用文本翻译 API 适配器。

use crate::error::ServiceError;
use crate::translate::config::TranslateProvider;
use crate::translate::engine::Translator;
use crate::translate::http::{network_error, shared_client};
use crate::translate::language::normalize_language;
use async_trait::async_trait;
use md5::{Digest, Md5};
use reqwest::Client;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// 百度通用文本翻译开放接口地址
const BAIDU_ENDPOINT: &str = "https://fanyi-api.baidu.com/api/trans/vip/translate";

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
}

#[derive(Deserialize)]
struct BaiduResponse {
    error_code: Option<String>,
    error_msg: Option<String>,
    trans_result: Option<Vec<BaiduTransItem>>,
}

#[derive(Deserialize)]
struct BaiduTransItem {
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

        // 百度支持通过换行符批量合并翻译
        let query = texts.join("\n");
        let salt = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let raw_sign = format!("{}{}{}{}", self.app_id, query, salt, self.secret_key);

        let mut hasher = Md5::new();
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
                ("appid", self.app_id.as_str()),
                ("salt", salt.as_str()),
                ("sign", sign.as_str()),
            ])
            .send()
            .await
            .map_err(|error| network_error("baidu", error))?;

        let resp_data: BaiduResponse = response
            .json()
            .await
            .map_err(|error| network_error("baidu", error))?;

        if let Some(err_code) = resp_data.error_code {
            if err_code != "52000" {
                return Err(ServiceError::ApiError {
                    provider: "baidu".to_string(),
                    code: err_code,
                    message: resp_data.error_msg.unwrap_or_default(),
                });
            }
        }

        let trans_items = resp_data
            .trans_result
            .ok_or_else(|| ServiceError::ApiError {
                provider: "baidu".to_string(),
                code: "EMPTY_RESULT".to_string(),
                message: "百度翻译接口未返回译文列表".to_string(),
            })?;
        let results: Vec<String> = trans_items.into_iter().map(|item| item.dst).collect();
        if results.len() != texts.len() {
            return Err(ServiceError::ApiError {
                provider: "baidu".to_string(),
                code: "RESULT_COUNT_MISMATCH".to_string(),
                message: format!(
                    "百度翻译返回 {} 条译文，与请求 {} 条不一致",
                    results.len(),
                    texts.len()
                ),
            });
        }
        Ok(results)
    }
}
