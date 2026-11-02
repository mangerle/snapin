//! 媒体任务分派、OCR 引擎缓存与翻译编排。
//!
//! 任务运行在统一 [`snapin_runtime::TaskRuntime`] 的异步上下文内；OCR 推理与
//! 图像排版通过 `spawn_blocking` 移出异步 worker。OCR 引擎按模型目录缓存，
//! 缓存锁只在阻塞线程内短暂持有，绝不跨越 `.await`。取消标志在 OCR、翻译、排版等
//! 阶段边界检查；单次 ONNX 推理本身不可中断，取消会丢弃其结果并阻止后续阶段。

use super::save::{execute_save_paster, execute_save_screenshot, SavePasterRequest};
use super::translate::{build_translation_items, to_service_provider, TranslationPlacement};
use super::OcrCache;
use snapin_config::config::Config;
use snapin_platform::pick_save_path;
use snapin_service::ocr::{LocalPpOcrEngine, OcrEngine, OcrTextBlock};
use snapin_service::qr::decode_qr_codes;
use snapin_service::{create_translator, Translator, TranslatorOptions};
use snapin_storage::{resolve_save_path, save_image_to_path, save_image_with_config};
use snapin_ui::{MediaJob, MediaJobOutcome, OcrExportFormat, OcrTextItem, QrCodeItem};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 执行一条媒体任务。
pub(super) async fn execute_job(
    job: MediaJob,
    config: Arc<Config>,
    cache: Arc<Mutex<OcrCache>>,
    cancel: Arc<AtomicBool>,
) -> MediaJobOutcome {
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    match job {
        MediaJob::Ocr { image, origin } => {
            execute_ocr(&cache, image, origin, &config, &cancel).await
        }
        MediaJob::DecodeQr { image, origin } => execute_qr(image, origin, &cancel).await,
        MediaJob::Translate {
            image,
            origin,
            cover_mode,
        } => {
            execute_translate(
                &cache,
                Arc::new(image),
                &config,
                TranslationPlacement { origin, cover_mode },
                &cancel,
            )
            .await
        }
        MediaJob::SaveScreenshot { image } => {
            execute_save_screenshot(Arc::new(image), &config).await
        }
        MediaJob::SavePaster {
            image,
            as_dialog,
            open_folder_after,
        } => {
            execute_save_paster(
                SavePasterRequest {
                    image: Arc::new(image),
                    as_dialog,
                    open_folder_after,
                },
                &config,
            )
            .await
        }
        MediaJob::SaveOcrText { lines, format } => {
            execute_save_ocr_text(lines, format, &config).await
        }
        MediaJob::SaveBeautified { image, options } => {
            execute_save_beautified(image, options, &config).await
        }
    }
}

/// 渲染美化截图并保存为当前默认输出格式。
async fn execute_save_beautified(
    image: image::RgbaImage,
    options: snapin_render::BeautifyOptions,
    config: &Config,
) -> MediaJobOutcome {
    let output = config.output.clone();
    let chosen_path = if output.prompt_save_path {
        let suggested = resolve_save_path(&output);
        let suggested_name = suggested
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("snapin.png")
            .to_string();
        match pick_save_path(&suggested_name, suggested.parent()).await {
            Ok(Some(path)) => Some(path),
            Ok(None) => return MediaJobOutcome::SaveCancelled,
            Err(error) => {
                return MediaJobOutcome::Failed {
                    message: format!("打开美化截图保存对话框失败: {error}"),
                }
            }
        }
    } else {
        None
    };
    let task = tokio::task::spawn_blocking(move || {
        let mut input = tiny_skia::Pixmap::new(image.width(), image.height())
            .ok_or_else(|| "分配美化输入缓冲区失败".to_string())?;
        input.data_mut().copy_from_slice(image.as_raw());
        let beautified = snapin_render::render_beautified(input.as_ref(), &options)?;
        let rgba =
            image::RgbaImage::from_raw(beautified.width(), beautified.height(), beautified.take())
                .ok_or_else(|| "转换美化输出图像失败".to_string())?;
        match chosen_path {
            Some(path) => save_image_to_path(&rgba, &path, &output),
            None => save_image_with_config(&rgba, &output),
        }
        .map_err(|error| format!("保存美化图像失败: {error}"))
    })
    .await;
    match task {
        Ok(Ok(path)) => MediaJobOutcome::Saved { path },
        Ok(Err(message)) => MediaJobOutcome::Failed { message },
        Err(error) => MediaJobOutcome::Failed {
            message: format!("美化渲染任务异常: {error}"),
        },
    }
}

/// 按用户选择的格式导出 OCR 文本。
async fn execute_save_ocr_text(
    lines: Vec<String>,
    format: OcrExportFormat,
    config: &Config,
) -> MediaJobOutcome {
    let output = config.output.clone();
    let result = tokio::task::spawn_blocking(move || {
        let (content, extension) = match format {
            OcrExportFormat::Markdown => (
                format!(
                    "# OCR 识别结果\n\n{}\n",
                    lines
                        .iter()
                        .map(|line| format!("- {line}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                "md",
            ),
            OcrExportFormat::Json => (
                serde_json::to_string_pretty(&lines).unwrap_or_else(|_| "[]".to_string()),
                "json",
            ),
        };
        snapin_storage::save_text_with_config(&content, extension, &output)
    })
    .await;

    match result {
        Ok(Ok(path)) => MediaJobOutcome::Saved { path },
        Ok(Err(error)) => MediaJobOutcome::Failed {
            message: format!("导出 OCR 文本失败: {error}"),
        },
        Err(error) => MediaJobOutcome::Failed {
            message: format!("OCR 文本导出任务异常: {error}"),
        },
    }
}

/// 获取或初始化 OCR 引擎。
///
/// 初始化在阻塞线程内执行并完成二次检查，避免多个任务同时加载同一模型；
/// 初始化失败同样写入缓存，用户修改模型目录后由新键触发重试。
async fn get_ocr_engine(
    cache: &Arc<Mutex<OcrCache>>,
    config: &Config,
) -> Result<Arc<LocalPpOcrEngine>, String> {
    let model_dir = config.ocr.local.model_dir.trim().to_string();
    let mut guard = cache.lock().await;
    if let Some((cached_dir, cached)) = &guard.entry {
        if cached_dir == &model_dir {
            return cached.clone();
        }
    }

    let init_dir = model_dir.clone();
    let initialized = tokio::task::spawn_blocking(move || {
        let custom_dir = if init_dir.is_empty() {
            None
        } else {
            Some(Path::new(&init_dir))
        };
        LocalPpOcrEngine::new(custom_dir)
            .map(Arc::new)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("OCR 引擎初始化任务异常: {error}"))?;
    guard.entry = Some((model_dir, initialized.clone()));
    initialized
}

/// 执行 OCR 并返回全文。
async fn execute_ocr(
    cache: &Arc<Mutex<OcrCache>>,
    image: image::RgbaImage,
    origin: (u32, u32),
    config: &Config,
    cancel: &Arc<AtomicBool>,
) -> MediaJobOutcome {
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    let engine = match get_ocr_engine(cache, config).await {
        Ok(engine) => engine,
        Err(message) => return MediaJobOutcome::Failed { message },
    };
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    let outcome = tokio::task::spawn_blocking(move || engine.recognize(&image)).await;
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    match outcome {
        Ok(Ok(result)) => {
            let lines = map_ocr_lines(result.blocks, origin);
            MediaJobOutcome::Ocr {
                text: result.full_text,
                lines,
            }
        }
        Ok(Err(error)) => MediaJobOutcome::Failed {
            message: error.to_string(),
        },
        Err(error) => MediaJobOutcome::Failed {
            message: format!("OCR 推理任务异常: {error}"),
        },
    }
}

/// 将服务层 OCR 文本块转换为 UI 可消费的强类型行数据。
fn map_ocr_lines(blocks: Vec<OcrTextBlock>, origin: (u32, u32)) -> Vec<OcrTextItem> {
    let mut lines = Vec::with_capacity(blocks.len());
    for block in blocks {
        let bounds = snapin_foundation::Rect::new(
            origin.0 as f32 + block.bbox.x,
            origin.1 as f32 + block.bbox.y,
            block.bbox.width,
            block.bbox.height,
        );
        lines.push(OcrTextItem {
            text: block.text,
            bounds,
        });
    }
    lines
}

/// 在阻塞线程池执行二维码检测，并统一映射为 UI 数据契约。
async fn execute_qr(
    image: image::RgbaImage,
    origin: (u32, u32),
    cancel: &Arc<AtomicBool>,
) -> MediaJobOutcome {
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    let task = tokio::task::spawn_blocking(move || decode_qr_codes(&image)).await;
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    match task {
        Ok(results) => {
            let mut codes = Vec::with_capacity(results.len());
            for result in results {
                codes.push(QrCodeItem {
                    text: result.text,
                    bounds: snapin_foundation::RectI::new(
                        origin.0.saturating_add(result.bounds.x),
                        origin.1.saturating_add(result.bounds.y),
                        result.bounds.width,
                        result.bounds.height,
                    ),
                });
            }
            MediaJobOutcome::QrCodes { codes }
        }
        Err(error) => MediaJobOutcome::Failed {
            message: format!("二维码检测任务异常: {error}"),
        },
    }
}

/// 执行 OCR、逐段翻译并生成原位翻译图元。
async fn execute_translate(
    cache: &Arc<Mutex<OcrCache>>,
    image: Arc<image::RgbaImage>,
    config: &Config,
    placement: TranslationPlacement,
    cancel: &Arc<AtomicBool>,
) -> MediaJobOutcome {
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    let engine = match get_ocr_engine(cache, config).await {
        Ok(engine) => engine,
        Err(message) => return MediaJobOutcome::Failed { message },
    };
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }

    let ocr_input = image.clone();
    let ocr_result = tokio::task::spawn_blocking(move || engine.recognize(&ocr_input)).await;
    let ocr_result = match ocr_result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            return MediaJobOutcome::Failed {
                message: error.to_string(),
            }
        }
        Err(error) => {
            return MediaJobOutcome::Failed {
                message: format!("OCR 推理任务异常: {error}"),
            }
        }
    };
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    if ocr_result.blocks.is_empty() {
        return MediaJobOutcome::Translated { items: Vec::new() };
    }

    let mut texts = Vec::with_capacity(ocr_result.blocks.len());
    for block in &ocr_result.blocks {
        texts.push(block.text.clone());
    }

    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    let translator = build_translator(config);
    let translated = translator
        .translate_batch(&texts, "auto", config.translate.target_language.as_str())
        .await;
    let translated = match translated {
        Ok(translated) => translated,
        Err(error) => {
            return MediaJobOutcome::Failed {
                message: error.to_string(),
            }
        }
    };
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }

    let layout = tokio::task::spawn_blocking(move || {
        build_translation_items(&image, &ocr_result.blocks, &translated, placement)
    })
    .await;
    match layout {
        Ok(items) => MediaJobOutcome::Translated { items },
        Err(error) => MediaJobOutcome::Failed {
            message: format!("译文排版任务异常: {error}"),
        },
    }
}

/// 用户主动取消后的统一结果；正常回调路径会因取消标志丢弃该结果。
fn cancelled() -> MediaJobOutcome {
    MediaJobOutcome::Failed {
        message: "任务已取消".to_string(),
    }
}

/// 按当前配置构建翻译器；底层 HTTP 客户端为进程级共享连接池，构建开销很低。
fn build_translator(config: &Config) -> Arc<dyn Translator> {
    let api = &config.translate.api_keys;
    create_translator(&TranslatorOptions {
        provider: to_service_provider(config.translate.provider),
        baidu_app_id: api.baidu_app_id.as_str().to_string(),
        baidu_secret_key: api.baidu_secret_key.as_str().to_string(),
        youdao_app_key: api.youdao_app_key.as_str().to_string(),
        youdao_app_secret: api.youdao_app_secret.as_str().to_string(),
        aliyun_access_key_id: api.aliyun_access_key_id.as_str().to_string(),
        aliyun_access_key_secret: api.aliyun_access_key_secret.as_str().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 相同模型目录必须命中已有缓存，不同目录必须触发重建。
    #[tokio::test]
    async fn test_ocr_cache_key_changes_with_model_dir() {
        let cache = Arc::new(Mutex::new(OcrCache::default()));
        let mut config = Config::default();
        config.ocr.local.model_dir = "/tmp/nonexistent-model-a".to_string();
        let first = get_ocr_engine(&cache, &config).await;
        assert!(first.is_err(), "不存在的模型目录必须返回错误");
        let cached = cache.lock().await.entry.clone();
        let (cached_dir, _) = cached.expect("错误结果也必须按目录缓存");
        assert_eq!(cached_dir, "/tmp/nonexistent-model-a");

        config.ocr.local.model_dir = "/tmp/nonexistent-model-b".to_string();
        let second = get_ocr_engine(&cache, &config).await;
        assert!(second.is_err());
        let cached = cache.lock().await.entry.clone();
        let (cached_dir, _) = cached.expect("新目录必须写入新缓存");
        assert_eq!(cached_dir, "/tmp/nonexistent-model-b");
    }
}
