//! 真实 PP-OCRv6 small 离线模型资产与端到端识别管线集成测试。

use image::RgbaImage;
use snapin_core::render::text::{TextBoxOptions, TextRenderer};
use snapin_service::ocr::{
    is_runtime_available, load_character_dict, LocalOcrPaths, LocalPpOcrEngine, OcrEngine,
};
use std::path::Path;
use tiny_skia::Pixmap;

#[test]
fn test_real_ppocrv6_assets_integrity_and_inference_pipeline() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let model_dir = workspace_root.join("assets").join("models").join("ppocrv6");

    if !model_dir.exists() || !model_dir.join("ppocrv6_small_rec.onnx").exists() {
        // 模型若未就绪则安全跳过，防止 CI 运行失败
        return;
    }
    if !is_runtime_available() {
        // 未安装系统 ONNX Runtime 时跳过真实推理，基础解析用例仍会执行
        return;
    }

    // 1. 验证模型路径自动探测与资产发现
    let paths = LocalOcrPaths::discover_in_root(&model_dir).expect("必须成功发现模型结构");
    assert!(
        paths.rec_model.exists(),
        "识别模型文件不存在: {:?}",
        paths.rec_model
    );
    assert!(
        paths.det_model.exists(),
        "检测模型文件不存在: {:?}",
        paths.det_model
    );
    assert!(
        paths.rec_config.exists(),
        "识别模型配置文件不存在: {:?}",
        paths.rec_config
    );

    // 2. 验证真实多语言字符字典反序列化完整度
    let dict = load_character_dict(&paths.rec_config).expect("必须成功加载多语言字典");
    assert!(
        dict.len() >= 18000,
        "PP-OCRv6 字符字典规模应不低于 18000，当前实际为: {}",
        dict.len()
    );
    assert!(dict.contains(&"A".to_string()));
    assert!(dict.contains(&"1".to_string()));

    // 3. 实例化真实本地推理引擎
    let engine = LocalPpOcrEngine::new(Some(&model_dir)).expect("本地引擎初始化失败");
    assert_eq!(engine.dict_len(), dict.len());

    // 4. 构造带有真实绘制文字的合成测试截图
    let width = 380;
    let height = 90;
    let mut pixmap = Pixmap::new(width, height).expect("分配测试画布失败");
    pixmap.fill(tiny_skia::Color::WHITE);

    let mut renderer = TextRenderer::new();
    renderer.draw_text_box_rgba(
        &mut pixmap.as_mut(),
        "Snapin OCR 2026",
        TextBoxOptions {
            rect: (12.0, 12.0, 356.0, 66.0),
            font_size: 28.0,
            rgba: (20, 20, 20, 255),
        },
    );

    let canvas = RgbaImage::from_raw(width, height, pixmap.data().to_vec())
        .expect("转换测试图像为 RGBA 失败");

    // 5. 执行端到端识别管线并断言真实文本内容
    let result = engine.recognize(&canvas).expect("识别推理管线执行失败");
    assert!(!result.blocks.is_empty(), "识别结果块列表不应为空");
    assert!(
        result.full_text.to_lowercase().contains("snapin"),
        "识别结果应包含 Snapin，实际: {:?}",
        result.full_text
    );

    let first_block = &result.blocks[0];
    assert!(first_block.confidence > 0.0, "识别置信度必须为正");
    assert_eq!(first_block.bg_color, (255, 255, 255, 255));
    assert_eq!(first_block.text_color, (24, 24, 27, 255));
}
