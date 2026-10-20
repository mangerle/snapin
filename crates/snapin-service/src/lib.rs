//! Snapin 服务层与多引擎调度系统。
//!
//! 提供基于 PP-OCRv6 的离线文字检测识别与五大多服务商（百度、网易有道、阿里、微软 Bing、谷歌）机器翻译集成。

pub mod error;
pub mod ocr;
pub mod translate;

pub use error::ServiceError;
pub use ocr::{
    is_runtime_available as is_ocr_runtime_available, load_character_dict, LocalOcrPaths,
    LocalPpOcrEngine, OcrEngine, OcrResult, OcrTextBlock,
};
pub use translate::{
    create_translator, TranslateProvider as ServiceTranslateProvider, Translator, TranslatorOptions,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_load_character_dict_from_txt() {
        let dir = tempdir().unwrap();
        let txt_path = dir.path().join("dict.txt");
        let mut file = std::fs::File::create(&txt_path).unwrap();
        writeln!(file, "你").unwrap();
        writeln!(file, "好").unwrap();
        writeln!(file, "世").unwrap();
        writeln!(file, "界").unwrap();

        let dict = load_character_dict(&txt_path).unwrap();
        assert_eq!(dict, vec!["你", "好", "世", "界"]);
    }

    #[test]
    fn test_load_character_dict_from_yml() {
        let dir = tempdir().unwrap();
        let yml_path = dir.path().join("inference.yml");
        let mut file = std::fs::File::create(&yml_path).unwrap();
        writeln!(file, "PostProcess:").unwrap();
        writeln!(file, "  character_dict:").unwrap();
        writeln!(file, "    - '0'").unwrap();
        writeln!(file, "    - 'A'").unwrap();
        writeln!(file, "    - '中'").unwrap();

        let dict = load_character_dict(&yml_path).unwrap();
        assert_eq!(dict, vec!["0", "A", "中"]);
    }

    #[test]
    fn test_load_character_dict_not_found() {
        let not_exist = std::path::Path::new("/path/to/nonexistent/dict.txt");
        let res = load_character_dict(not_exist);
        assert!(matches!(res, Err(ServiceError::ModelNotFound { .. })));
    }

    #[test]
    fn test_load_character_dict_invalid_format() {
        let dir = tempdir().unwrap();
        let invalid_yml = dir.path().join("inference.yml");
        std::fs::write(&invalid_yml, "Invalid: [Yaml Content").unwrap();
        let res = load_character_dict(&invalid_yml);
        assert!(matches!(res, Err(ServiceError::ConfigParseError { .. })));
    }

    #[test]
    fn test_create_translator_maps_provider() {
        let cases = [
            (ServiceTranslateProvider::BingFree, "bing_free"),
            (ServiceTranslateProvider::GoogleFree, "google_free"),
            (ServiceTranslateProvider::Baidu, "baidu"),
            (ServiceTranslateProvider::Youdao, "youdao"),
            (ServiceTranslateProvider::Aliyun, "aliyun"),
        ];

        for (provider, expected_name) in cases {
            let translator = create_translator(&TranslatorOptions {
                provider,
                ..TranslatorOptions::default()
            });
            assert_eq!(translator.name(), expected_name);
        }
    }

    #[tokio::test]
    async fn test_translators_empty_batch() {
        let bing = translate::providers::BingFreeTranslator::new();
        assert_eq!(
            bing.translate_batch(&[], "auto", "zh").await.unwrap(),
            Vec::<String>::new()
        );

        let google = translate::providers::GoogleFreeTranslator::new();
        assert_eq!(
            google.translate_batch(&[], "auto", "zh").await.unwrap(),
            Vec::<String>::new()
        );
    }

    #[tokio::test]
    async fn test_translators_missing_credentials() {
        let baidu = translate::providers::BaiduTranslator::new(String::new(), String::new());
        let res = baidu
            .translate_batch(&["hello".to_string()], "en", "zh")
            .await;
        assert!(
            matches!(res, Err(ServiceError::MissingCredentials { provider, .. }) if provider == "baidu")
        );

        let youdao = translate::providers::YoudaoTranslator::new(String::new(), String::new());
        let res = youdao
            .translate_batch(&["hello".to_string()], "en", "zh")
            .await;
        assert!(
            matches!(res, Err(ServiceError::MissingCredentials { provider, .. }) if provider == "youdao")
        );

        let aliyun = translate::providers::AliyunTranslator::new(String::new(), String::new());
        let res = aliyun
            .translate_batch(&["hello".to_string()], "en", "zh")
            .await;
        assert!(
            matches!(res, Err(ServiceError::MissingCredentials { provider, .. }) if provider == "aliyun")
        );
    }

    #[test]
    fn test_real_model_loading_and_pipeline() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        let model_dir = workspace_root.join("assets").join("models").join("ppocrv6");

        if !model_dir.exists() || !model_dir.join("ppocrv6_small_rec.onnx").exists() {
            // 模型文件尚不存在时安全返回
            return;
        }
        if !crate::is_ocr_runtime_available() {
            // 无系统 ONNX Runtime 的开发机允许跳过真实推理用例
            return;
        }

        // 1. 验证路径自动探测
        let paths = LocalOcrPaths::discover_in_root(&model_dir).expect("应成功探测到模型结构");
        assert!(paths.rec_model.exists());
        assert!(paths.det_model.exists());
        assert!(paths.rec_config.exists());

        // 2. 验证字符字典加载（PP-OCRv6 字典规模超过 18,000 字符）
        let dict = load_character_dict(&paths.rec_config).expect("应成功解析实际模型字典");
        assert!(
            dict.len() > 18000,
            "字典规模应大于 18000，实际: {}",
            dict.len()
        );

        // 3. 验证本地引擎初始化
        let engine = LocalPpOcrEngine::new(Some(&model_dir)).expect("应成功基于真实模型实例化引擎");
        assert_eq!(engine.dict_len(), dict.len());

        // 4. 构造包含真实文字的合成截图并验证识别文本内容
        let mut pixmap = tiny_skia::Pixmap::new(380, 90).expect("分配测试画布失败");
        pixmap.fill(tiny_skia::Color::WHITE);
        let mut renderer = snapin_core::render::TextRenderer::new();
        renderer.draw_text_box_rgba(
            &mut pixmap.as_mut(),
            "Snapin OCR 2026",
            snapin_core::render::TextBoxOptions {
                rect: (12.0, 12.0, 356.0, 66.0),
                font_size: 28.0,
                rgba: (20, 20, 20, 255),
            },
        );
        let sample_img =
            image::RgbaImage::from_raw(380, 90, pixmap.take()).expect("转换测试图像为 RGBA 失败");

        let res = engine.recognize(&sample_img).expect("识别管线执行应成功");
        assert!(!res.blocks.is_empty(), "应返回识别文本块");
        let merged = res.full_text.to_lowercase();
        assert!(
            merged.contains("snapin"),
            "识别结果应包含 Snapin，实际: {:?}",
            res.full_text
        );
        assert!(
            res.blocks.iter().all(|block| block.confidence > 0.0),
            "文本块置信度必须为正"
        );
    }
}
