//! 真实 PP-OCRv6 small 离线模型资产与端到端识别管线集成测试。

use image::RgbaImage;
use snapin_core::render::text::{TextBoxOptions, TextRenderer};
use snapin_service::ocr::{load_character_dict, LocalOcrPaths, LocalPpOcrEngine, OcrEngine};
use std::path::Path;
use tiny_skia::Pixmap;

#[tokio::test]
async fn test_real_ppocrv6_assets_integrity_and_inference_pipeline() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let model_dir = workspace_root.join("assets").join("models").join("ppocrv6");

    if !model_dir.exists() || !model_dir.join("ppocrv6_small_rec.onnx").exists() {
        // 模型若未就绪则安全跳过，防止 CI 运行失败
        return;
    }

    // 1. 验证模型路径三级回退与资产发现
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
    println!("成功加载 PP-OCRv6 字典，字符总数: {}", dict.len());
    assert!(
        dict.len() >= 18000,
        "PP-OCRv6 字符字典规模应不低于 18000，当前实际为: {}",
        dict.len()
    );
    // 验证包含常见英文字符与汉字
    assert!(dict.contains(&"A".to_string()));
    assert!(dict.contains(&"1".to_string()));

    // 3. 实例化真实本地推理引擎
    let engine = LocalPpOcrEngine::new(Some(&model_dir)).expect("本地引擎初始化失败");
    assert_eq!(engine.dict_len(), dict.len());

    // 4. 构造带有真实绘制文字的合成测试截图
    let width = 360;
    let height = 80;
    let mut pixmap = Pixmap::new(width, height).unwrap();
    pixmap.fill(tiny_skia::Color::WHITE);

    let mut renderer = TextRenderer::new();
    let text_options = TextBoxOptions {
        rect: (10.0, 10.0, 340.0, 60.0),
        font_size: 24.0,
        rgba: (30, 30, 30, 255),
    };
    renderer.draw_text_box_rgba(&mut pixmap.as_mut(), "Snapin 截图识别", text_options);

    let canvas = RgbaImage::from_raw(width, height, pixmap.data().to_vec()).unwrap();

    // 5. 执行端到端识别管线
    let result = engine
        .recognize(&canvas)
        .await
        .expect("识别推理管线执行失败");
    assert!(!result.blocks.is_empty(), "识别结果块列表不应为空");

    let first_block = &result.blocks[0];
    // 验证背景色取色（纯白）与文字对比色（高对比锌黑）
    assert_eq!(first_block.bg_color, (255, 255, 255, 255));
    assert_eq!(first_block.text_color, (24, 24, 27, 255));
    assert_eq!(first_block.polygon.len(), 4);
}
