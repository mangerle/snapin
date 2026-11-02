//! 翻译图元排版与应用配置到服务层枚举的映射。

use snapin_annotation::shapes::TranslationShapeItem;
use snapin_annotation::Rect;
use snapin_config::config::{CoverMode, TranslateProvider};
use snapin_service::ocr::OcrTextBlock;
use snapin_service::ocr::{estimate_dominant_background_color, get_contrasting_text_color};
use snapin_service::ServiceTranslateProvider;

/// 翻译图元在目标图像中的放置参数。
#[derive(Debug, Clone, Copy)]
pub(super) struct TranslationPlacement {
    /// 识别结果相对目标图像的像素原点
    pub(super) origin: (u32, u32),
    /// 原位背景覆盖模式
    pub(super) cover_mode: CoverMode,
}

/// 将核心配置的翻译服务商映射为服务层独立枚举。
///
/// 服务层刻意不依赖 `snapin-config`，两层配置模型在应用边界完成一次显式转换；
/// 这样服务 crate 可以独立测试与复用，而 UI 不需要知道两套枚举的存在。
pub(super) fn to_service_provider(provider: TranslateProvider) -> ServiceTranslateProvider {
    match provider {
        TranslateProvider::BingFree => ServiceTranslateProvider::BingFree,
        TranslateProvider::GoogleFree => ServiceTranslateProvider::GoogleFree,
        TranslateProvider::Baidu => ServiceTranslateProvider::Baidu,
        TranslateProvider::Youdao => ServiceTranslateProvider::Youdao,
        TranslateProvider::Aliyun => ServiceTranslateProvider::Aliyun,
    }
}

/// 根据识别块与译文构建原位翻译图元，过滤空译文与原文相同的项。
pub(super) fn build_translation_items(
    image: &image::RgbaImage,
    blocks: &[OcrTextBlock],
    translated: &[String],
    placement: TranslationPlacement,
) -> Vec<TranslationShapeItem> {
    let mut items = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate() {
        let Some(translated) = translated.get(index) else {
            continue;
        };
        let translated = translated.trim();
        if translated.is_empty() || translated == block.text.trim() {
            continue;
        }

        let bg_color = estimate_dominant_background_color(image, block.bbox);
        let text_color = get_contrasting_text_color(bg_color);
        let font_size = (block.bbox.height.max(2.0) * 0.75).clamp(10.0, 36.0);

        items.push(TranslationShapeItem {
            rect: Rect::new(
                placement.origin.0 as f32 + block.bbox.x,
                placement.origin.1 as f32 + block.bbox.y,
                block.bbox.width.max(2.0),
                block.bbox.height.max(2.0),
            ),
            original: block.text.clone(),
            translated: translated.to_string(),
            bg_color: bg_color.into(),
            text_color: text_color.into(),
            font_size,
            cover_mode: placement.cover_mode,
        });
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapin_foundation::{Point, Rect};

    /// 翻译图元必须过滤空译文与原文相同的段落，并按原点平移坐标。
    #[test]
    fn test_build_translation_items_filters_identical_and_offsets() {
        let image = image::RgbaImage::from_pixel(200, 100, image::Rgba([255, 255, 255, 255]));
        let blocks = vec![
            OcrTextBlock::new(
                "Hello".to_string(),
                Rect::new(10.0, 10.0, 40.0, 20.0),
                [Point::new(0.0, 0.0); 4],
                0.9,
            ),
            OcrTextBlock::new(
                "World".to_string(),
                Rect::new(10.0, 40.0, 40.0, 20.0),
                [Point::new(0.0, 0.0); 4],
                0.9,
            ),
            OcrTextBlock::new(
                "Snapin".to_string(),
                Rect::new(10.0, 70.0, 40.0, 20.0),
                [Point::new(0.0, 0.0); 4],
                0.9,
            ),
        ];
        let translated = vec!["你好".to_string(), String::new(), "Snapin".to_string()];

        let items = build_translation_items(
            &image,
            &blocks,
            &translated,
            TranslationPlacement {
                origin: (100, 50),
                cover_mode: CoverMode::SolidColor,
            },
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].translated, "你好");
        assert_eq!(items[0].rect.x, 110.0);
        assert_eq!(items[0].rect.y, 60.0);
    }

    /// 服务商映射必须覆盖全部变体，避免新增枚举后静默落到默认值。
    #[test]
    fn test_provider_mapping_is_total() {
        let cases = [
            (
                TranslateProvider::BingFree,
                ServiceTranslateProvider::BingFree,
            ),
            (
                TranslateProvider::GoogleFree,
                ServiceTranslateProvider::GoogleFree,
            ),
            (TranslateProvider::Baidu, ServiceTranslateProvider::Baidu),
            (TranslateProvider::Youdao, ServiceTranslateProvider::Youdao),
            (TranslateProvider::Aliyun, ServiceTranslateProvider::Aliyun),
        ];
        for (core, service) in cases {
            assert_eq!(to_service_provider(core), service);
        }
    }
}
