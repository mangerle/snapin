//! 翻译服务商 HTTP 响应契约集成测试。
//!
//! 各 provider 的端点在构造时注入本地一次性 Mock 服务，从而在不联网的前提下锁定
//! 成功解析、业务错误码、条数不匹配、凭据缺失与限流降级这五类真实会发生的分支行为。

use crate::error::ServiceError;
use crate::translate::engine::Translator;
use crate::translate::providers::{
    AliyunTranslator, BaiduTranslator, BingFreeTranslator, GoogleFreeTranslator, YoudaoTranslator,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

/// 状态码与响应体组成的预置应答
type CannedResponse = (u16, &'static str);

/// 本地一次性 HTTP Mock 服务。
///
/// 在工作线程上按序受理固定数量的连接并返回预置应答，应答耗尽后监听套接字随线程结束释放，
/// 因此不残留端口与线程；每个请求都回 `Connection: close`，避免 keep-alive 复用打乱计数。
struct MockHttpServer {
    base_url: String,
}

impl MockHttpServer {
    fn start(responses: Vec<CannedResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定本地 Mock 端口成功");
        let port = listener
            .local_addr()
            .expect("读取 Mock 监听端口成功")
            .port();

        std::thread::spawn(move || {
            for (status, body) in responses {
                let Ok((mut stream, _addr)) = listener.accept() else {
                    break;
                };
                if let Err(error) = drain_request(&mut stream) {
                    tracing::debug!("Mock 服务读取请求失败，忽略该连接: {error}");
                    continue;
                }
                let head = format!(
                    "HTTP/1.1 {status} MOCK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body.as_bytes());
                let _ = stream.flush();
            }
        });

        Self {
            base_url: format!("http://127.0.0.1:{port}"),
        }
    }

    /// 拼接指向本 Mock 的完整端点
    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

/// 读取并丢弃请求头与请求体，让客户端不会因连接提前关闭而报错
fn drain_request(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut content_length = 0usize;

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(())
}

/// 必应翻译页面片段：包含 IG 与防滥用令牌，供 Mock 服务作为首个响应
const BING_PAGE_BODY: &str = r#"<html><script>var IG:"AA11BB22CC33",_G={};params_AbusePreventionHelper = [1792548490247,"tok123",3600000];</script></html>"#;

/// 构造测试用文本列表
fn texts(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

#[tokio::test]
async fn test_baidu_parses_batch_result_in_request_order() {
    let mock = MockHttpServer::start(vec![(
        200,
        r#"{"trans_result":[{"src":"apple","dst":"苹果"},{"src":"pen","dst":"钢笔"}]}"#,
    )]);
    let translator = BaiduTranslator::with_endpoint(
        mock.endpoint("/api/trans/vip/translate"),
        "app-id".to_string(),
        "secret".to_string(),
    );

    let result = translator
        .translate_batch(&texts(&["apple", "pen"]), "en", "zh")
        .await
        .expect("百度批量翻译应成功");
    assert_eq!(result, texts(&["苹果", "钢笔"]));
}

#[tokio::test]
async fn test_baidu_maps_business_error_code() {
    let mock = MockHttpServer::start(vec![(
        200,
        r#"{"error_code":"54003","error_msg":"Invalid User"}"#,
    )]);
    let translator = BaiduTranslator::with_endpoint(
        mock.endpoint("/api/trans/vip/translate"),
        "app-id".to_string(),
        "secret".to_string(),
    );

    let error = translator
        .translate_batch(&texts(&["apple"]), "en", "zh")
        .await
        .expect_err("业务错误码必须转换为错误");
    match error {
        ServiceError::ApiError {
            provider,
            code,
            message,
        } => {
            assert_eq!(provider, "baidu");
            assert_eq!(code, "54003");
            assert_eq!(message, "Invalid User");
        }
        other => panic!("应返回服务商业务错误，实际: {other:?}"),
    }
}

#[tokio::test]
async fn test_baidu_rejects_result_count_mismatch() {
    let mock = MockHttpServer::start(vec![(
        200,
        r#"{"trans_result":[{"src":"apple","dst":"苹果"}]}"#,
    )]);
    let translator = BaiduTranslator::with_endpoint(
        mock.endpoint("/api/trans/vip/translate"),
        "app-id".to_string(),
        "secret".to_string(),
    );

    let error = translator
        .translate_batch(&texts(&["apple", "pen"]), "en", "zh")
        .await
        .expect_err("译文条数不一致必须报错，否则会静默丢行");
    match error {
        ServiceError::ApiError { code, message, .. } => {
            assert_eq!(code, "RESULT_COUNT_MISMATCH");
            assert!(
                message.contains("不一致"),
                "错误信息应说明条数不一致，实际: {message}"
            );
        }
        other => panic!("应返回服务商业务错误，实际: {other:?}"),
    }
}

#[tokio::test]
async fn test_baidu_requires_credentials_before_network() {
    let translator = BaiduTranslator::new(String::new(), "secret".to_string());
    let error = translator
        .translate_batch(&texts(&["apple"]), "en", "zh")
        .await
        .expect_err("凭据缺失必须提前失败");
    assert!(
        matches!(error, ServiceError::MissingCredentials { ref provider, .. } if provider == "baidu"),
        "应返回凭据缺失错误，实际: {error:?}"
    );
}

#[tokio::test]
async fn test_youdao_splits_merged_translation_by_lines() {
    let mock = MockHttpServer::start(vec![(
        200,
        r#"{"errorCode":"0","translation":["苹果\n钢笔"]}"#,
    )]);
    let translator = YoudaoTranslator::with_endpoint(
        mock.endpoint("/api"),
        "app-key".to_string(),
        "app-secret".to_string(),
    );

    let result = translator
        .translate_batch(&texts(&["apple", "pen"]), "en", "zh")
        .await
        .expect("有道合并译文应按行还原为逐条结果");
    assert_eq!(result, texts(&["苹果", "钢笔"]));
}

#[tokio::test]
async fn test_youdao_maps_non_zero_error_code() {
    let mock = MockHttpServer::start(vec![(200, r#"{"errorCode":"102"}"#)]);
    let translator = YoudaoTranslator::with_endpoint(
        mock.endpoint("/api"),
        "app-key".to_string(),
        "app-secret".to_string(),
    );

    let error = translator
        .translate_batch(&texts(&["apple"]), "en", "zh")
        .await
        .expect_err("非零 errorCode 必须转换为错误");
    match error {
        ServiceError::ApiError { provider, code, .. } => {
            assert_eq!(provider, "youdao");
            assert_eq!(code, "102");
        }
        other => panic!("应返回服务商业务错误，实际: {other:?}"),
    }
}

#[tokio::test]
async fn test_aliyun_parses_nested_data_and_error_code() {
    let ok_mock = MockHttpServer::start(vec![(
        200,
        r#"{"Code":"200","Data":{"Translated":"苹果\n钢笔"}}"#,
    )]);
    let translator =
        AliyunTranslator::with_endpoint(ok_mock.endpoint("/"), "ak".to_string(), "sk".to_string());
    let result = translator
        .translate_batch(&texts(&["apple", "pen"]), "en", "zh")
        .await
        .expect("阿里云嵌套结构应正确解包");
    assert_eq!(result, texts(&["苹果", "钢笔"]));

    let err_mock = MockHttpServer::start(vec![(
        200,
        r#"{"Code":"10003","Message":"Unsupported target language"}"#,
    )]);
    let failing =
        AliyunTranslator::with_endpoint(err_mock.endpoint("/"), "ak".to_string(), "sk".to_string());
    let error = failing
        .translate_batch(&texts(&["apple"]), "en", "fr")
        .await
        .expect_err("非 200 Code 必须转换为错误");
    match error {
        ServiceError::ApiError { code, message, .. } => {
            assert_eq!(code, "10003");
            assert_eq!(message, "Unsupported target language");
        }
        other => panic!("应返回服务商业务错误，实际: {other:?}"),
    }
}

#[tokio::test]
async fn test_bing_free_preserves_order_under_concurrency() {
    // 首个响应是令牌页面，其后为逐条翻译响应；乱序完成也必须按请求下标回填
    let mock = MockHttpServer::start(vec![
        (200, BING_PAGE_BODY),
        (200, r#"[{"translations":[{"text":"苹果"}]}]"#),
        (200, r#"[{"translations":[{"text":"钢笔"}]}]"#),
    ]);
    let translator = BingFreeTranslator::with_endpoint(
        mock.endpoint("/ttranslatev3"),
        mock.endpoint("/translator"),
    );

    let result = translator
        .translate_batch(&texts(&["apple", "pen"]), "auto", "zh-CN")
        .await
        .expect("必应免费接口应返回逐条译文");
    assert_eq!(result, texts(&["苹果", "钢笔"]));
}

/// 出口 IP 被风控时接口返回 ShowCaptcha，必须如实上报而不是伪装成空译文
#[tokio::test]
async fn test_bing_free_reports_captcha_challenge() {
    let mock = MockHttpServer::start(vec![
        (200, BING_PAGE_BODY),
        (200, r#"{"ShowCaptcha":true}"#),
    ]);
    let translator = BingFreeTranslator::with_endpoint(
        mock.endpoint("/ttranslatev3"),
        mock.endpoint("/translator"),
    );

    let error = translator
        .translate_batch(&texts(&["apple"]), "auto", "zh-CN")
        .await
        .expect_err("风控响应必须转换为错误");
    match error {
        ServiceError::ApiError {
            provider,
            code,
            message,
        } => {
            assert_eq!(provider, "bing_free");
            assert_eq!(code, "SHOW_CAPTCHA");
            assert!(
                message.contains("风控"),
                "错误信息应说明被风控，实际: {message}"
            );
        }
        other => panic!("应返回服务商业务错误，实际: {other:?}"),
    }
}

/// 风控页返回 HTML 时必须给出可操作的中文错误，而不是底层 JSON 解码失败
#[tokio::test]
async fn test_bing_free_reports_non_json_response() {
    let mock = MockHttpServer::start(vec![
        (200, BING_PAGE_BODY),
        (200, "<html>captcha challenge</html>"),
    ]);
    let translator = BingFreeTranslator::with_endpoint(
        mock.endpoint("/ttranslatev3"),
        mock.endpoint("/translator"),
    );

    let error = translator
        .translate_batch(&texts(&["apple"]), "auto", "zh-CN")
        .await
        .expect_err("非 JSON 风控页必须转换为业务错误");
    match error {
        ServiceError::ApiError { code, message, .. } => {
            assert_eq!(code, "NON_JSON_RESPONSE");
            assert!(
                message.contains("谷歌"),
                "错误信息应给出可用替代通道，实际: {message}"
            );
        }
        other => panic!("应返回服务商业务错误，实际: {other:?}"),
    }
}

/// 页面结构变更导致取不到令牌时，错误应指向令牌解析失败而非笼统空结果
#[tokio::test]
async fn test_bing_free_reports_token_unavailable() {
    let mock = MockHttpServer::start(vec![(200, "<html>no tokens here</html>")]);
    let translator = BingFreeTranslator::with_endpoint(
        mock.endpoint("/ttranslatev3"),
        mock.endpoint("/translator"),
    );

    let error = translator
        .translate_batch(&texts(&["apple"]), "auto", "zh-CN")
        .await
        .expect_err("缺少令牌必须报错");
    match error {
        ServiceError::ApiError { code, .. } => assert_eq!(code, "TOKEN_UNAVAILABLE"),
        other => panic!("应返回服务商业务错误，实际: {other:?}"),
    }
}

#[tokio::test]
async fn test_bing_free_reports_http_status_when_blocked() {
    let mock = MockHttpServer::start(vec![(200, BING_PAGE_BODY), (429, "")]);
    let translator = BingFreeTranslator::with_endpoint(
        mock.endpoint("/ttranslatev3"),
        mock.endpoint("/translator"),
    );

    let error = translator
        .translate_batch(&texts(&["apple"]), "auto", "zh-CN")
        .await
        .expect_err("限流状态码必须转换为错误");
    match error {
        ServiceError::ApiError { provider, code, .. } => {
            assert_eq!(provider, "bing_free");
            assert!(
                code.starts_with("429"),
                "错误码应携带限流状态码，实际: {code}"
            );
        }
        other => panic!("应返回服务商业务错误，实际: {other:?}"),
    }
}

#[tokio::test]
async fn test_google_free_batches_in_one_request() {
    let mock = MockHttpServer::start(vec![(200, r#"["苹果","钢笔"]"#)]);
    let translator = GoogleFreeTranslator::with_endpoint(mock.endpoint("/translate_a/t"));

    let result = translator
        .translate_batch(&texts(&["apple", "pen"]), "en", "zh-CN")
        .await
        .expect("谷歌免费接口应一次请求返回批量译文");
    assert_eq!(result, texts(&["苹果", "钢笔"]));
}

/// 自动检测源语言时接口返回 [译文, 检测语言] 嵌套数组，必须与扁平形态同样解析
#[tokio::test]
async fn test_google_free_parses_auto_detect_nested_payload() {
    let mock = MockHttpServer::start(vec![(200, r#"[["早上好","en"]]"#)]);
    let translator = GoogleFreeTranslator::with_endpoint(mock.endpoint("/translate_a/t"));

    let result = translator
        .translate_batch(&texts(&["good morning"]), "auto", "zh-CN")
        .await
        .expect("自动检测模式应正确取到译文");
    assert_eq!(result, texts(&["早上好"]));
}

#[tokio::test]
async fn test_google_free_fails_on_empty_payload() {
    let mock = MockHttpServer::start(vec![(200, r#"[""]"#)]);
    let translator = GoogleFreeTranslator::with_endpoint(mock.endpoint("/translate_a/t"));

    let error = translator
        .translate_batch(&texts(&["apple"]), "auto", "zh-CN")
        .await
        .expect_err("空译文响应必须转换为错误而非静默空串");
    match error {
        ServiceError::ApiError { code, .. } => assert_eq!(code, "EMPTY_RESULT"),
        other => panic!("应返回服务商业务错误，实际: {other:?}"),
    }
}

/// 主端点被出口网络拦截时必须自动切换到备用端点，而不是直接失败
#[tokio::test]
async fn test_google_free_falls_back_to_second_endpoint() {
    let primary = MockHttpServer::start(vec![(500, "blocked")]);
    let backup = MockHttpServer::start(vec![(200, r#"["苹果"]"#)]);
    let translator = GoogleFreeTranslator::with_endpoints(vec![
        primary.endpoint("/translate_a/t"),
        backup.endpoint("/translate_a/t"),
    ]);

    let result = translator
        .translate_batch(&texts(&["apple"]), "en", "zh-CN")
        .await
        .expect("主端点失败后应使用备用端点");
    assert_eq!(result, texts(&["苹果"]));
}

#[tokio::test]
async fn test_all_providers_return_empty_batch_without_network() {
    // 空文本列表必须短路返回，绝不发起请求（此处不提供任何 Mock 应答即可验证）
    let cases: Vec<(&str, Box<dyn Translator>)> = vec![
        (
            "baidu",
            Box::new(BaiduTranslator::new("id".to_string(), "secret".to_string())),
        ),
        (
            "youdao",
            Box::new(YoudaoTranslator::new(
                "key".to_string(),
                "secret".to_string(),
            )),
        ),
        (
            "aliyun",
            Box::new(AliyunTranslator::new("ak".to_string(), "sk".to_string())),
        ),
        ("bing_free", Box::new(BingFreeTranslator::new())),
        ("google_free", Box::new(GoogleFreeTranslator::new())),
    ];

    for (name, translator) in cases {
        let result = translator
            .translate_batch(&[], "auto", "zh-CN")
            .await
            .unwrap_or_else(|error| panic!("{name} 空批次不应访问网络，实际失败: {error}"));
        assert!(result.is_empty(), "{name} 空批次应返回空结果");
    }
}

/// 真实网络连通性验证（默认不参与 CI，需显式 `--ignored` 运行）。
///
/// 免费接口的可用性取决于出口网络策略，无法在离线 CI 中断言，故标记忽略；
/// 本地排查"翻译用不了"时应先跑此用例确认是接口被拦截还是代码解析问题。
#[tokio::test]
#[ignore = "依赖真实外网，默认跳过"]
async fn live_google_free_endpoint_reachable() {
    let translator = GoogleFreeTranslator::new();
    let result = translator
        .translate_batch(&texts(&["hello world"]), "en", "zh-CN")
        .await
        .expect("谷歌免费接口应可用；若失败说明当前出口网络被拦截");
    assert_eq!(result.len(), 1);
    assert!(!result[0].is_empty());
}
