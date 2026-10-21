//! 截图会话后台任务调度与结果回传。
//!
//! OCR 推理、网络翻译与重排版计算从 winit 事件循环中剥离，统一投递到 Tokio 后台任务执行；
//! 任务完成后通过 `EventLoopProxy` 以用户事件形式回传 UI 线程，避免界面冻结并支持取消。

use super::state::SnipperState;
use super::types::SnipperOutcome;
use snapin_core::annotation::action::AnnotationShape;
use snapin_core::annotation::shapes::{TranslationAction, TranslationShapeItem};
use snapin_core::config::{Config, CoverMode};
use snapin_platform::{save_image_with_config, write_image_to_clipboard, write_text_to_clipboard};
use snapin_service::ocr::{
    estimate_dominant_background_color, get_contrasting_text_color, LocalPpOcrEngine, OcrEngine,
    OcrResult, OcrTextBlock,
};
use snapin_service::{create_translator, ServiceTranslateProvider, Translator, TranslatorOptions};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tokio::runtime::Handle;
use tokio::task::AbortHandle;
use winit::event_loop::EventLoopProxy;

/// 截图会话支持的后台任务类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnipperJobKind {
    Ocr,
    Translate,
    Save,
}

impl SnipperJobKind {
    /// 中文任务名称，用于界面提示
    pub fn label(self) -> &'static str {
        match self {
            Self::Ocr => "文字识别",
            Self::Translate => "原位翻译",
            Self::Save => "保存截图",
        }
    }
}

/// 投递到后台的截图任务
pub struct SnipperJob {
    pub kind: SnipperJobKind,
    /// 已完成标注合成的选区裁剪图像
    pub image: image::RgbaImage,
    /// 选区在全屏图像中的像素原点
    pub origin: (u32, u32),
    /// 原位翻译背景覆盖模式
    pub cover_mode: CoverMode,
}

/// 后台任务执行结果
#[derive(Debug)]
pub enum SnipperJobOutcome {
    Ocr(OcrResult),
    Translate(Vec<TranslationShapeItem>),
    Saved {
        path: std::path::PathBuf,
        image: image::RgbaImage,
    },
    Failed {
        kind: SnipperJobKind,
        message: String,
    },
}

/// 发送回 winit 事件循环的用户事件
#[derive(Debug)]
pub enum SnipperUserEvent {
    JobFinished(SnipperJobOutcome),
}

/// 会话级服务容器：复用 OCR 引擎、翻译器及其网络连接池。
struct AppServices {
    config: Arc<Config>,
    ocr_engine: OnceLock<Result<Arc<LocalPpOcrEngine>, String>>,
    translator: OnceLock<Arc<dyn Translator>>,
}

impl AppServices {
    fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            ocr_engine: OnceLock::new(),
            translator: OnceLock::new(),
        }
    }

    fn ocr_engine(&self) -> Result<Arc<LocalPpOcrEngine>, String> {
        self.ocr_engine
            .get_or_init(|| {
                let model_dir = self.config.ocr.local.model_dir.trim();
                let custom_dir = if model_dir.is_empty() {
                    None
                } else {
                    Some(Path::new(model_dir))
                };
                LocalPpOcrEngine::new(custom_dir)
                    .map(Arc::new)
                    .map_err(|error| error.to_string())
            })
            .clone()
    }

    fn translator(&self) -> Arc<dyn Translator> {
        self.translator
            .get_or_init(|| create_translator(&translator_options(&self.config.translate)))
            .clone()
    }
}

/// 将应用配置映射为服务层独立的翻译器参数。
///
/// 贴图工作区的整图翻译复用同一映射，避免两处配置解释出现分叉。
pub(crate) fn translator_options(
    config: &snapin_core::config::TranslateConfig,
) -> TranslatorOptions {
    let provider = match config.provider {
        snapin_core::config::TranslateProvider::BingFree => ServiceTranslateProvider::BingFree,
        snapin_core::config::TranslateProvider::GoogleFree => ServiceTranslateProvider::GoogleFree,
        snapin_core::config::TranslateProvider::Baidu => ServiceTranslateProvider::Baidu,
        snapin_core::config::TranslateProvider::Youdao => ServiceTranslateProvider::Youdao,
        snapin_core::config::TranslateProvider::Aliyun => ServiceTranslateProvider::Aliyun,
    };

    TranslatorOptions {
        provider,
        baidu_app_id: config.api_keys.baidu_app_id.clone(),
        baidu_secret_key: config.api_keys.baidu_secret_key.clone(),
        youdao_app_key: config.api_keys.youdao_app_key.clone(),
        youdao_app_secret: config.api_keys.youdao_app_secret.clone(),
        aliyun_access_key_id: config.api_keys.aliyun_access_key_id.clone(),
        aliyun_access_key_secret: config.api_keys.aliyun_access_key_secret.clone(),
    }
}

/// 后台任务调度器：持有事件循环代理与会话级服务。
pub struct SnipperJobDispatcher {
    proxy: EventLoopProxy<SnipperUserEvent>,
    services: Arc<AppServices>,
}

impl SnipperJobDispatcher {
    /// 构建任务调度器
    pub fn new(proxy: EventLoopProxy<SnipperUserEvent>, config: Arc<Config>) -> Self {
        Self {
            proxy,
            services: Arc::new(AppServices::new(config)),
        }
    }

    /// 投递任务到 Tokio 运行时；返回可取消的任务句柄。
    pub fn dispatch(&self, job: SnipperJob) -> Option<AbortHandle> {
        let handle = Handle::try_current().ok()?;
        let proxy = self.proxy.clone();
        let services = self.services.clone();

        let join_handle = handle.spawn(async move {
            let outcome = execute_job(services, job).await;
            let _ = proxy.send_event(SnipperUserEvent::JobFinished(outcome));
        });

        Some(join_handle.abort_handle())
    }
}

async fn execute_job(services: Arc<AppServices>, job: SnipperJob) -> SnipperJobOutcome {
    match job.kind {
        SnipperJobKind::Ocr => {
            let result = tokio::task::spawn_blocking(move || {
                let engine = services.ocr_engine()?;
                engine
                    .recognize(&job.image)
                    .map_err(|error| error.to_string())
            })
            .await;

            match result {
                Ok(Ok(result)) => SnipperJobOutcome::Ocr(result),
                Ok(Err(message)) => SnipperJobOutcome::Failed {
                    kind: SnipperJobKind::Ocr,
                    message,
                },
                Err(error) => SnipperJobOutcome::Failed {
                    kind: SnipperJobKind::Ocr,
                    message: format!("推理任务异常: {error}"),
                },
            }
        }
        SnipperJobKind::Save => {
            let config = services.config.clone();
            let result = tokio::task::spawn_blocking(move || {
                save_with_fallback(&job.image, &config).map(|path| (path, job.image))
            })
            .await;

            match result {
                Ok(Ok((path, image))) => SnipperJobOutcome::Saved { path, image },
                Ok(Err(message)) => SnipperJobOutcome::Failed {
                    kind: SnipperJobKind::Save,
                    message,
                },
                Err(error) => SnipperJobOutcome::Failed {
                    kind: SnipperJobKind::Save,
                    message: format!("保存任务异常: {error}"),
                },
            }
        }
        SnipperJobKind::Translate => {
            let SnipperJob {
                image,
                origin,
                cover_mode,
                ..
            } = job;
            let ocr_services = services.clone();
            let ocr_result = tokio::task::spawn_blocking(move || {
                let engine = ocr_services.ocr_engine()?;
                let result = engine
                    .recognize(&image)
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>((result, image))
            })
            .await;

            let (ocr_result, image) = match ocr_result {
                Ok(Ok(pair)) => pair,
                Ok(Err(message)) => {
                    return SnipperJobOutcome::Failed {
                        kind: SnipperJobKind::Translate,
                        message,
                    }
                }
                Err(error) => {
                    return SnipperJobOutcome::Failed {
                        kind: SnipperJobKind::Translate,
                        message: format!("推理任务异常: {error}"),
                    }
                }
            };

            if ocr_result.blocks.is_empty() {
                return SnipperJobOutcome::Translate(Vec::new());
            }

            let texts: Vec<String> = ocr_result
                .blocks
                .iter()
                .map(|block| block.text.clone())
                .collect();
            let translator = services.translator();
            let target_language = services.config.translate.target_language.as_str();
            let translated = match translator
                .translate_batch(&texts, "auto", target_language)
                .await
            {
                Ok(translated) => translated,
                Err(error) => {
                    return SnipperJobOutcome::Failed {
                        kind: SnipperJobKind::Translate,
                        message: error.to_string(),
                    }
                }
            };

            let items = tokio::task::spawn_blocking(move || {
                build_translation_items(&image, &ocr_result.blocks, &translated, origin, cover_mode)
            })
            .await;

            match items {
                Ok(items) => SnipperJobOutcome::Translate(items),
                Err(error) => SnipperJobOutcome::Failed {
                    kind: SnipperJobKind::Translate,
                    message: format!("译文排版任务异常: {error}"),
                },
            }
        }
    }
}

/// 按用户配置保存截图；配置路径失败时降级保存到 Pictures 目录。
fn save_with_fallback(
    image: &image::RgbaImage,
    config: &Config,
) -> Result<std::path::PathBuf, String> {
    if let Ok(path) = save_image_with_config(image, &config.output) {
        return Ok(path);
    }

    let pictures_dir = snapin_core::AppPaths::pictures_dir();
    std::fs::create_dir_all(&pictures_dir)
        .map_err(|error| format!("创建 Pictures 目录失败: {error}"))?;
    let fallback_path = pictures_dir.join(format!(
        "snapin_{}.png",
        snapin_core::AppPaths::generate_unique_id()
    ));
    image
        .save(&fallback_path)
        .map_err(|error| format!("降级保存截图失败: {error}"))?;
    Ok(fallback_path)
}

/// 根据识别块与译文构建原位翻译图元，过滤空译文与原文相同的项。
///
/// 贴图窗口的整图翻译同样复用该函数，仅将图元原点固定为 `(0, 0)`。
pub(crate) fn build_translation_items(
    image: &image::RgbaImage,
    blocks: &[OcrTextBlock],
    translated: &[String],
    origin: (u32, u32),
    cover_mode: CoverMode,
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

        let (bx, by, bw, bh) = block.bbox;
        let bg_color = estimate_dominant_background_color(image, block.bbox);
        let text_color = get_contrasting_text_color(bg_color);
        let font_size = (bh.max(2.0) * 0.75).clamp(10.0, 36.0);

        items.push(TranslationShapeItem {
            rect: (
                origin.0 as f32 + bx,
                origin.1 as f32 + by,
                bw.max(2.0),
                bh.max(2.0),
            ),
            original: block.text.clone(),
            translated: translated.to_string(),
            bg_color,
            text_color,
            font_size,
            cover_mode,
        });
    }
    items
}

/// 处理后台任务结果，驱动截图会话状态流转。
pub fn handle_job_outcome(state: &mut SnipperState, outcome: SnipperJobOutcome) {
    state.pending_job = None;
    state.pending_job_abort = None;

    match outcome {
        SnipperJobOutcome::Ocr(result) => {
            let text = result.full_text.trim().to_string();
            if text.is_empty() {
                state.set_status_message("未识别到文字");
                return;
            }
            match write_text_to_clipboard(&text) {
                Ok(()) => {
                    // 识别结果只写入剪贴板，不结束截图会话：用户常常要在同一张图上继续
                    // 标注或再识别另一块区域，直接关闭会让人来不及确认结果。
                    state.set_status_message(format!(
                        "已复制识别文字（{} 字符）",
                        text.chars().count()
                    ));
                }
                Err(error) => {
                    tracing::warn!("复制 OCR 文本至系统剪贴板失败: {error}");
                    state.set_status_message(format!("复制识别文字失败: {error}"));
                }
            }
        }
        SnipperJobOutcome::Translate(items) => {
            if items.is_empty() {
                state.set_status_message("未获得有效翻译结果");
                return;
            }
            state
                .annotations
                .push(AnnotationShape::Translation(TranslationAction::new(items)));
            state.toolbar.active_tool = None;
            state.toolbar.sub_rect = None;
            state.clear_status_message();
            tracing::info!("原位翻译完成，已成功压入标注动作栈");
        }
        SnipperJobOutcome::Saved { path, image } => {
            if let Err(error) = write_image_to_clipboard(&image) {
                tracing::warn!("保存后同步复制到系统剪贴板失败: {error}");
            }
            state.outcome = Some(SnipperOutcome::Saved(path));
        }
        SnipperJobOutcome::Failed { kind, message } => {
            tracing::warn!("{}后台任务失败: {message}", kind.label());
            state.set_status_message(format!("{}失败: {message}", kind.label()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn block(text: &str, bbox: (f32, f32, f32, f32)) -> OcrTextBlock {
        OcrTextBlock::new(
            text.to_string(),
            bbox,
            [
                (bbox.0, bbox.1),
                (bbox.0 + bbox.2, bbox.1),
                (bbox.0 + bbox.2, bbox.1 + bbox.3),
                (bbox.0, bbox.1 + bbox.3),
            ],
            0.9,
        )
    }

    #[test]
    fn test_handle_empty_ocr_outcome_keeps_session_open() {
        let mut state = SnipperState::new();
        handle_job_outcome(&mut state, SnipperJobOutcome::Ocr(OcrResult::default()));
        assert!(state.outcome.is_none());
        assert_eq!(state.status_message.as_deref(), Some("未识别到文字"));
    }

    #[test]
    fn test_handle_translate_outcome_pushes_annotation() {
        let mut state = SnipperState::new();
        let item = TranslationShapeItem {
            rect: (10.0, 10.0, 40.0, 20.0),
            original: "Hello".to_string(),
            translated: "你好".to_string(),
            bg_color: (255, 255, 255, 255),
            text_color: (0, 0, 0, 255),
            font_size: 14.0,
            cover_mode: CoverMode::SolidColor,
        };
        handle_job_outcome(&mut state, SnipperJobOutcome::Translate(vec![item]));
        assert!(state.annotations.can_undo());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_handle_saved_outcome_sets_result() {
        let mut state = SnipperState::new();
        let path = std::path::PathBuf::from("/tmp/snapin_test.png");
        let image = image::RgbaImage::from_pixel(2, 2, Rgba([255, 255, 255, 255]));
        handle_job_outcome(
            &mut state,
            SnipperJobOutcome::Saved {
                path: path.clone(),
                image,
            },
        );
        match state.outcome {
            Some(SnipperOutcome::Saved(saved_path)) => assert_eq!(saved_path, path),
            _ => panic!("保存成功必须返回 Saved 结果"),
        }
    }

    #[test]
    fn test_handle_job_failure_sets_status() {
        let mut state = SnipperState::new();
        handle_job_outcome(
            &mut state,
            SnipperJobOutcome::Failed {
                kind: SnipperJobKind::Translate,
                message: "网络超时".to_string(),
            },
        );
        assert!(state
            .status_message
            .as_deref()
            .unwrap_or_default()
            .contains("网络超时"));
    }

    #[test]
    fn test_build_translation_items_filters_empty_and_identical() {
        let image = image::RgbaImage::from_pixel(200, 100, Rgba([255, 255, 255, 255]));
        let blocks = vec![
            block("Hello", (10.0, 10.0, 40.0, 20.0)),
            block("World", (10.0, 40.0, 40.0, 20.0)),
            block("Snapin", (10.0, 70.0, 40.0, 20.0)),
        ];
        let translated = vec!["你好".to_string(), String::new(), "Snapin".to_string()];

        let items = build_translation_items(
            &image,
            &blocks,
            &translated,
            (100, 50),
            CoverMode::SolidColor,
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].translated, "你好");
        assert_eq!(items[0].rect.0, 110.0);
        assert_eq!(items[0].rect.1, 60.0);
    }
}

#[cfg(test)]
mod ocr_outcome_tests {
    use super::*;
    use crate::snipper::window::state::SnipperState;
    use snapin_service::ocr::OcrResult;

    /// OCR 成功后应保留截图会话并给出结果提示，而不是立刻关闭窗口
    #[test]
    fn test_ocr_success_keeps_session_open() {
        let mut state = SnipperState::new();
        let result = OcrResult {
            full_text: "识别出来的一段文字".to_string(),
            ..Default::default()
        };

        handle_job_outcome(&mut state, SnipperJobOutcome::Ocr(result));

        assert!(state.outcome.is_none(), "OCR 成功不得直接把会话置为结束态");
        let message = state.status_message.as_deref().unwrap_or_default();
        assert!(
            message.contains("已复制识别文字"),
            "应提示识别结果已复制，实际: {message}"
        );
    }
}
