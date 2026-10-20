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
    // 并发批量请求按文本逐条发出，Mock 依次应答；乱序完成也必须按请求下标回填
    let mock = MockHttpServer::start(vec![
        (200, r#"[{"translations":[{"text":"苹果"}]}]"#),
        (200, r#"[{"translations":[{"text":"钢笔"}]}]"#),
    ]);
    let translator = BingFreeTranslator::with_endpoint(mock.endpoint("/ttranslatev3"));

    let result = translator
        .translate_batch(&texts(&["apple", "pen"]), "auto", "zh-CN")
        .await
        .expect("必应免费接口应返回逐条译文");
    assert_eq!(result, texts(&["苹果", "钢笔"]));
}

#[tokio::test]
async fn test_bing_free_reports_http_status_when_blocked() {
    let mock = MockHttpServer::start(vec![(429, "")]);
    let translator = BingFreeTranslator::with_endpoint(mock.endpoint("/ttranslatev3"));

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
async fn test_google_free_concatenates_sentence_segments() {
    let mock = MockHttpServer::start(vec![(
        200,
        r#"[[["苹果","apple",null,null,10]],null,"zh-CN"]"#,
    )]);
    let translator = GoogleFreeTranslator::with_endpoint(mock.endpoint("/translate_a/single"));

    let result = translator
        .translate_batch(&texts(&["apple"]), "auto", "zh-CN")
        .await
        .expect("谷歌免费接口应拼接分句译文");
    assert_eq!(result, texts(&["苹果"]));
}

#[tokio::test]
async fn test_google_free_fails_on_empty_payload() {
    let mock = MockHttpServer::start(vec![(200, "[[]]")]);
    let translator = GoogleFreeTranslator::with_endpoint(mock.endpoint("/translate_a/single"));

    let error = translator
        .translate_batch(&texts(&["apple"]), "auto", "zh-CN")
        .await
        .expect_err("空译文响应必须转换为错误而非静默空串");
    match error {
        ServiceError::ApiError { code, .. } => assert_eq!(code, "EMPTY_RESULT"),
        other => panic!("应返回服务商业务错误，实际: {other:?}"),
    }
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
