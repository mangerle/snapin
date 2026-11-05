//! 媒体任务分派、OCR 引擎缓存与翻译编排。
//!
//! 任务运行在统一 [`snapin_runtime::TaskRuntime`] 的异步上下文内；OCR 推理与
//! 图像排版通过统一运行时的 `run_blocking_detached` 移出异步 worker。OCR 引擎按模型目录缓存，
//! 目录映射只在短临界区内取用，模型加载由 `OnceCell` 在锁外完成，同一目录的
//! 并发任务共享一次初始化。取消标志在 OCR、翻译、排版等阶段边界检查；单次
//! ONNX 推理本身不可中断，取消会丢弃其结果并阻止后续阶段。

use super::save::{execute_save_paster, execute_save_screenshot, SavePasterRequest};
use super::translate::{build_translation_items, to_service_provider, TranslationPlacement};
use super::OcrCache;
use snapin_config::config::Config;
use snapin_platform::pick_save_path;
use snapin_runtime::TaskRuntime;
use snapin_service::ocr::{LocalPpOcrEngine, OcrEngine, OcrResult, OcrTextBlock};
use snapin_service::qr::decode_qr_codes;
use snapin_service::{create_translator, Translator, TranslatorOptions};
use snapin_storage::{
    ensure_output_directory, save_image_to_path, save_image_with_config, suggest_save_path,
};
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
    runtime: TaskRuntime,
) -> MediaJobOutcome {
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    let context = MediaTaskContext {
        cache: &cache,
        config: &config,
        cancel: &cancel,
        runtime: &runtime,
    };
    match job {
        MediaJob::Ocr { image, origin } => execute_ocr(&context, image, origin).await,
        MediaJob::DecodeQr { image, origin } => execute_qr(image, origin, &cancel, &runtime).await,
        MediaJob::Translate {
            image,
            origin,
            cover_mode,
        } => {
            execute_translate(
                &context,
                Arc::new(image),
                TranslationPlacement { origin, cover_mode },
            )
            .await
        }
        MediaJob::Beautify { image, options } => execute_beautify(image, options, &runtime).await,
        MediaJob::SaveScreenshot { image } => {
            execute_save_screenshot(Arc::new(image), &config, &runtime).await
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
                &runtime,
            )
            .await
        }
        MediaJob::SaveOcrText { lines, format } => {
            execute_save_ocr_text(lines, format, &config, &runtime).await
        }
        MediaJob::SaveBeautified { image, options } => {
            execute_save_beautified(image, options, &config, &runtime).await
        }
    }
}

/// OCR 与翻译执行器共享的缓存、配置和取消上下文。
#[derive(Clone, Copy)]
struct MediaTaskContext<'a> {
    cache: &'a Arc<Mutex<OcrCache>>,
    config: &'a Config,
    cancel: &'a Arc<AtomicBool>,
    runtime: &'a TaskRuntime,
}

/// 渲染美化截图并保存为当前默认输出格式。
async fn execute_save_beautified(
    image: image::RgbaImage,
    options: snapin_render::BeautifyOptions,
    config: &Config,
    runtime: &TaskRuntime,
) -> MediaJobOutcome {
    let output = config.output.clone();
    let chosen_path = if output.prompt_save_path {
        let dialog_output = output.clone();
        let suggestion = runtime
            .run_blocking_detached(move || {
                if let Err(error) = ensure_output_directory(&dialog_output) {
                    tracing::warn!(
                        "准备美化截图保存目录失败，保存对话框将使用系统默认位置: {error}"
                    );
                }
                suggest_save_path(&dialog_output)
            })
            .await;
        let suggestion = match suggestion {
            Ok(suggestion) => suggestion,
            Err(error) => {
                return MediaJobOutcome::Failed {
                    message: format!("准备美化截图保存建议失败: {error}"),
                }
            }
        };
        match pick_save_path(&suggestion.file_name, Some(suggestion.directory.as_path())).await {
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
    let rgba = match beautify_image(image, options, runtime).await {
        Ok(image) => image,
        Err(message) => return MediaJobOutcome::Failed { message },
    };
    let task = runtime
        .run_blocking_detached(move || match chosen_path {
            Some(path) => save_image_to_path(&rgba, &path, &output),
            None => save_image_with_config(&rgba, &output),
        })
        .await;
    match task {
        Ok(Ok(path)) => MediaJobOutcome::Saved { path },
        Ok(Err(error)) => MediaJobOutcome::Failed {
            message: format!("保存美化图像失败: {error}"),
        },
        Err(error) => MediaJobOutcome::Failed {
            message: format!("保存美化图像任务异常: {error}"),
        },
    }
}

/// 在后台合成美化图像，供复制与贴图流程复用。
async fn execute_beautify(
    image: image::RgbaImage,
    options: snapin_render::BeautifyOptions,
    runtime: &TaskRuntime,
) -> MediaJobOutcome {
    match beautify_image(image, options, runtime).await {
        Ok(image) => MediaJobOutcome::Beautified { image },
        Err(message) => MediaJobOutcome::Failed { message },
    }
}

/// 在阻塞线程中执行美化渲染，返回可直接写入剪贴板或贴图窗口的 RGBA 图像。
async fn beautify_image(
    image: image::RgbaImage,
    options: snapin_render::BeautifyOptions,
    runtime: &TaskRuntime,
) -> Result<image::RgbaImage, String> {
    runtime
        .run_blocking_detached(move || {
            let mut input = tiny_skia::Pixmap::new(image.width(), image.height())
                .ok_or_else(|| "分配美化输入缓冲区失败".to_string())?;
            input.data_mut().copy_from_slice(image.as_raw());
            let beautified = snapin_render::render_beautified(input.as_ref(), &options)
                .map_err(|error| format!("美化渲染失败: {error}"))?;
            image::RgbaImage::from_raw(beautified.width(), beautified.height(), beautified.take())
                .ok_or_else(|| "转换美化输出图像失败".to_string())
        })
        .await
        .map_err(|error| format!("美化渲染任务异常: {error}"))?
}

/// 按用户选择的格式导出 OCR 文本。
async fn execute_save_ocr_text(
    lines: Vec<String>,
    format: OcrExportFormat,
    config: &Config,
    runtime: &TaskRuntime,
) -> MediaJobOutcome {
    let output = config.output.clone();
    let result = runtime
        .run_blocking_detached(move || {
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
/// 缓存锁只用于取用“目录 → 初始化单元”的映射，不覆盖模型加载；同一模型目录的
/// 并发任务共享同一个 `OnceCell`，由它保证初始化只执行一次。初始化失败同样
/// 写入该单元，用户修改模型目录后由新目录触发新的初始化。
async fn get_ocr_engine(
    cache: &Arc<Mutex<OcrCache>>,
    config: &Config,
    runtime: &TaskRuntime,
) -> Result<Arc<LocalPpOcrEngine>, String> {
    let model_dir = config.ocr.local.model_dir.trim().to_string();
    let cell = cache.lock().await.slot(&model_dir);
    let runtime = runtime.clone();
    let initialized = cell
        .get_or_init(|| async move {
            let init_dir = model_dir.clone();
            match runtime
                .run_blocking_detached(move || {
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
            {
                Ok(result) => result,
                Err(error) => Err(format!("OCR 引擎初始化任务异常: {error}")),
            }
        })
        .await;
    initialized.clone()
}

/// 执行 OCR 并返回全文。
async fn execute_ocr(
    context: &MediaTaskContext<'_>,
    image: image::RgbaImage,
    origin: (u32, u32),
) -> MediaJobOutcome {
    if context.cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    let engine = match get_ocr_engine(context.cache, context.config, context.runtime).await {
        Ok(engine) => engine,
        Err(message) => return MediaJobOutcome::Failed { message },
    };
    if context.cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    let outcome = context
        .runtime
        .run_blocking_detached(move || engine.recognize(&image))
        .await;
    if context.cancel.load(Ordering::Acquire) {
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
    runtime: &TaskRuntime,
) -> MediaJobOutcome {
    if cancel.load(Ordering::Acquire) {
        return cancelled();
    }
    let task = runtime
        .run_blocking_detached(move || decode_qr_codes(&image))
        .await;
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
    context: &MediaTaskContext<'_>,
    image: Arc<image::RgbaImage>,
    placement: TranslationPlacement,
) -> MediaJobOutcome {
    let ocr_result = match recognize_for_translation(context, image.clone()).await {
        Ok(result) => result,
        Err(outcome) => return outcome,
    };
    if ocr_result.blocks.is_empty() {
        return MediaJobOutcome::Translated { items: Vec::new() };
    }
    let translated = match translate_blocks(context, &ocr_result).await {
        Ok(translated) => translated,
        Err(outcome) => return outcome,
    };
    layout_translation(image, ocr_result, translated, placement, context.runtime).await
}

/// 在取消边界内执行 OCR，并将推理错误转换为最终任务回执。
async fn recognize_for_translation(
    context: &MediaTaskContext<'_>,
    image: Arc<image::RgbaImage>,
) -> Result<OcrResult, MediaJobOutcome> {
    if context.cancel.load(Ordering::Acquire) {
        return Err(cancelled());
    }
    let engine = get_ocr_engine(context.cache, context.config, context.runtime)
        .await
        .map_err(|message| MediaJobOutcome::Failed { message })?;
    if context.cancel.load(Ordering::Acquire) {
        return Err(cancelled());
    }
    let result = context
        .runtime
        .run_blocking_detached(move || engine.recognize(&image))
        .await;
    if context.cancel.load(Ordering::Acquire) {
        return Err(cancelled());
    }
    match result {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) => Err(MediaJobOutcome::Failed {
            message: error.to_string(),
        }),
        Err(error) => Err(MediaJobOutcome::Failed {
            message: format!("OCR 推理任务异常: {error}"),
        }),
    }
}

/// 提取 OCR 文本并调用配置的翻译服务。
async fn translate_blocks(
    context: &MediaTaskContext<'_>,
    ocr_result: &OcrResult,
) -> Result<Vec<String>, MediaJobOutcome> {
    let mut texts = Vec::with_capacity(ocr_result.blocks.len());
    for block in &ocr_result.blocks {
        texts.push(block.text.clone());
    }
    if context.cancel.load(Ordering::Acquire) {
        return Err(cancelled());
    }
    let translator = build_translator(context.config);
    let translated = translator
        .translate_batch(
            &texts,
            "auto",
            context.config.translate.target_language.as_str(),
        )
        .await
        .map_err(|error| MediaJobOutcome::Failed {
            message: error.to_string(),
        })?;
    if context.cancel.load(Ordering::Acquire) {
        return Err(cancelled());
    }
    Ok(translated)
}

/// 在阻塞线程池中生成原位翻译图元并映射任务结果。
async fn layout_translation(
    image: Arc<image::RgbaImage>,
    ocr_result: OcrResult,
    translated: Vec<String>,
    placement: TranslationPlacement,
    runtime: &TaskRuntime,
) -> MediaJobOutcome {
    let layout = runtime
        .run_blocking_detached(move || {
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
        let runtime = TaskRuntime::current(1).expect("测试运行时必须可用");
        let cache = Arc::new(Mutex::new(OcrCache::default()));
        let mut config = Config::default();
        config.ocr.local.model_dir = "/tmp/nonexistent-model-a".to_string();
        let first = get_ocr_engine(&cache, &config, &runtime).await;
        assert!(first.is_err(), "不存在的模型目录必须返回错误");
        let cached = cache.lock().await.entry.clone();
        let (cached_dir, _) = cached.expect("错误结果也必须按目录缓存");
        assert_eq!(cached_dir, "/tmp/nonexistent-model-a");

        config.ocr.local.model_dir = "/tmp/nonexistent-model-b".to_string();
        let second = get_ocr_engine(&cache, &config, &runtime).await;
        assert!(second.is_err());
        let cached = cache.lock().await.entry.clone();
        let (cached_dir, _) = cached.expect("新目录必须写入新缓存");
        assert_eq!(cached_dir, "/tmp/nonexistent-model-b");
    }
}
