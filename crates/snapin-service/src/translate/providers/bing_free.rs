//! 免费微软必应翻译适配器（免 Key 开放接口）。
//!
//! 必应接口不接受裸请求：直接 POST `ttranslatev3` 只会得到 `{"statusCode":400}`。
//! 必须先访问翻译页面取得会话令牌（`IG`、`params_AbusePreventionHelper` 中的 key/token）
//! 并带上该次访问下发的 Cookie，再发起翻译请求，因此客户端必须启用 Cookie 存储。
//!
//! 翻译请求必须发往令牌页面最终落地的同一区域域名：`www.bing.com` 会按出口地区
//! 302 到 `cn.bing.com` 等镜像，若仍向 `www` 发 POST，重定向会把请求改写成 GET
//! 并返回 HTML，表现为解码失败。出口 IP 被风控时接口会返回 `ShowCaptcha` 或
//! 非 JSON 的风控页，此时如实上报为业务错误而不是空译文。

use crate::error::ServiceError;
use crate::translate::config::TranslateProvider;
use crate::translate::engine::Translator;
use crate::translate::http::{network_error, shared_client};
use crate::translate::language::normalize_language;
use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde_json::Value;

/// 必应免费翻译接口地址
const BING_ENDPOINT: &str = "https://www.bing.com/ttranslatev3";

/// 令牌所在页面
const BING_PAGE: &str = "https://www.bing.com/translator";

/// 接口要求的浏览器标识，缺失时会被直接拒绝
const BING_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";

/// 页面内嵌的接口版本标识，与 IG 一同放入查询串
const BING_IID: &str = "translator.5025";

/// 单次翻译会话：复用页面访问建立的连接、区域端点与防滥用令牌
#[derive(Clone)]
struct BingSession {
    client: Client,
    endpoint: String,
    referer: String,
    ig: String,
    key: String,
    token: String,
}

/// 免费微软必应翻译服务适配器
pub struct BingFreeTranslator {
    client: Client,
    endpoint: String,
    page: String,
}

impl Default for BingFreeTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl BingFreeTranslator {
    /// 构造新的必应翻译适配器实例（复用进程级 HTTP 连接池）
    pub fn new() -> Self {
        Self::with_endpoint(BING_ENDPOINT, BING_PAGE)
    }

    /// 以指定端点构造适配器，供集成测试注入本地 Mock 服务校验响应契约。
    pub(crate) fn with_endpoint(endpoint: impl Into<String>, page: impl Into<String>) -> Self {
        Self {
            client: shared_client(),
            endpoint: endpoint.into(),
            page: page.into(),
        }
    }

    /// 抓取页面并解析出本次会话所需的 IG 与防滥用令牌
    async fn fetch_session(&self) -> Result<BingSession, ServiceError> {
        let response = self
            .client
            .get(self.page.as_str())
            .header(reqwest::header::USER_AGENT, BING_USER_AGENT)
            .send()
            .await
            .map_err(|error| network_error("bing_free", error))?;
        // 页面可能按出口地区重定向（如 www → cn），翻译请求必须跟随到同一区域域名
        let endpoint = regional_endpoint(response.url(), &self.endpoint);
        let referer = response.url().to_string();
        let html = response
            .text()
            .await
            .map_err(|error| network_error("bing_free", error))?;

        let ig = extract_between(&html, "IG:\"", '"').ok_or_else(|| ServiceError::ApiError {
            provider: "bing_free".to_string(),
            code: "TOKEN_UNAVAILABLE".to_string(),
            message: "未能从必应翻译页面解析出 IG 令牌，页面结构可能已变更".to_string(),
        })?;

        let helper =
            extract_between(&html, "params_AbusePreventionHelper = [", ']').ok_or_else(|| {
                ServiceError::ApiError {
                    provider: "bing_free".to_string(),
                    code: "TOKEN_UNAVAILABLE".to_string(),
                    message: "未能从必应翻译页面解析出防滥用令牌".to_string(),
                }
            })?;

        let mut parts = helper.split(',');
        let key = parts
            .next()
            .map(|value| value.trim().trim_matches('"').to_string())
            .unwrap_or_default();
        let token = parts
            .next()
            .map(|value| value.trim().trim_matches('"').to_string())
            .unwrap_or_default();
        if key.is_empty() || token.is_empty() {
            return Err(ServiceError::ApiError {
                provider: "bing_free".to_string(),
                code: "TOKEN_UNAVAILABLE".to_string(),
                message: "必应防滥用令牌格式异常".to_string(),
            });
        }

        Ok(BingSession {
            client: self.client.clone(),
            endpoint,
            referer,
            ig: ig.to_string(),
            key,
            token,
        })
    }

    /// 翻译单条文本
    async fn translate_one(
        session: &BingSession,
        text: &str,
        from_lang: Option<&str>,
        to_lang: &str,
    ) -> Result<String, ServiceError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(String::new());
        }

        let url = format!(
            "{}?isVertical=1&IG={}&IID={BING_IID}",
            session.endpoint, session.ig
        );

        let mut form: Vec<(&str, &str)> = vec![("to", to_lang), ("text", trimmed)];
        if let Some(from_lang) = from_lang {
            form.push(("fromLang", from_lang));
        }
        form.push(("token", session.token.as_str()));
        form.push(("key", session.key.as_str()));

        let response = session
            .client
            .post(url)
            .header(reqwest::header::USER_AGENT, BING_USER_AGENT)
            .header(reqwest::header::REFERER, session.referer.as_str())
            .form(&form)
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

        let body = response
            .text()
            .await
            .map_err(|error| network_error("bing_free", error))?;
        let json: Value = serde_json::from_str(&body).map_err(|_| ServiceError::ApiError {
            provider: "bing_free".to_string(),
            code: "NON_JSON_RESPONSE".to_string(),
            message: "必应翻译接口返回了非 JSON 响应（常见于浏览器风控页），当前网络无法使用必应免费通道，建议改用谷歌免费通道".to_string(),
        })?;

        if let Some(status) = json.get("statusCode").and_then(Value::as_i64) {
            return Err(ServiceError::ApiError {
                provider: "bing_free".to_string(),
                code: status.to_string(),
                message: format!(
                    "必应翻译接口拒绝了本次请求（HTTP {status}），会话令牌失效或非浏览器客户端被风控，建议改用谷歌免费通道"
                ),
            });
        }
        if json
            .get("ShowCaptcha")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(ServiceError::ApiError {
                provider: "bing_free".to_string(),
                code: "SHOW_CAPTCHA".to_string(),
                message: "必应接口触发人机校验：当前出口 IP 被风控，建议改用谷歌免费通道"
                    .to_string(),
            });
        }

        let translated = json
            .as_array()
            .and_then(|array| array.first())
            .and_then(|item| item.get("translations"))
            .and_then(Value::as_array)
            .and_then(|translations| translations.first())
            .and_then(|first| first.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

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

/// 将翻译端点切换到页面最终落地的区域域名，保留协议、端口与路径。
///
/// 必应会把 `www.bing.com` 按出口地区重定向到 `cn.bing.com` 等镜像；令牌与 Cookie
/// 都属于该次区域会话，继续向原域名发 POST 会被 302 改写为 GET 并返回 HTML。
fn regional_endpoint(page_url: &reqwest::Url, endpoint: &str) -> String {
    let Ok(mut target) = reqwest::Url::parse(endpoint) else {
        return endpoint.to_string();
    };
    let switched = page_url
        .host_str()
        .map(|host| target.set_host(Some(host)).is_ok())
        .unwrap_or(false);
    if switched {
        target.to_string()
    } else {
        endpoint.to_string()
    }
}

/// 从页面文本中截取两个标记之间的内容
fn extract_between<'a>(haystack: &'a str, start_marker: &str, end_char: char) -> Option<&'a str> {
    let start = haystack.find(start_marker)? + start_marker.len();
    let rest = &haystack[start..];
    let end = rest.find(end_char)?;
    Some(&rest[..end])
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

        // 令牌按会话下发，整批翻译共用一次，避免逐条重复抓取页面
        let session = self.fetch_session().await?;

        let results = stream::iter(texts.iter().cloned().enumerate())
            .map(|(index, text)| {
                let session = session.clone();
                let from_lang = from_lang.clone();
                let to_lang = to_lang.clone();
                async move {
                    let result =
                        Self::translate_one(&session, &text, from_lang.as_deref(), &to_lang).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 翻译端点必须跟随令牌页面的区域域名，同时保留端口与路径
    #[test]
    fn test_regional_endpoint_follows_page_host() {
        let page =
            reqwest::Url::parse("http://127.0.0.1:45678/translator").expect("页面地址解析失败");
        assert_eq!(
            regional_endpoint(&page, "http://127.0.0.1:45678/ttranslatev3"),
            "http://127.0.0.1:45678/ttranslatev3"
        );

        let cn = reqwest::Url::parse("https://cn.bing.com/translator").expect("页面地址解析失败");
        assert_eq!(
            regional_endpoint(&cn, BING_ENDPOINT),
            "https://cn.bing.com/ttranslatev3"
        );
    }
}
