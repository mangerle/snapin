//! 媒体任务分派、OCR 引擎缓存与翻译编排。
//!
//! 任务运行在统一 [`snapin_runtime::TaskRuntime`] 的异步上下文内；OCR 推理与
//! 图像排版通过 `spawn_blocking` 移出异步 worker。OCR 引擎按模型目录缓存，
//! 缓存锁只在阻塞线程内短暂持有，绝不跨越 `.await`。取消标志在 OCR、翻译、排版等
//! 阶段边界检查；单次 ONNX 推理本身不可中断，取消会丢弃其结果并阻止后续阶段。

use super::save::{execute_save_paster, execute_save_screenshot, SavePasterRequest};
use super::translate::{build_translation_items, to_service_provider, TranslationPlacement};
use super::OcrCache;
use snapin_core::config::Config;
use snapin_service::ocr::{LocalPpOcrEngine, OcrEngine};
use snapin_service::{create_translator, Translator, TranslatorOptions};
use snapin_ui::{MediaJob, MediaJobOutcome};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
        MediaJob::Ocr { image } => execute_ocr(&cache, image, &config, &cancel).await,
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
    {
        let guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((cached_dir, cached)) = &guard.entry {
            if cached_dir == &model_dir {
                return cached.clone();
            }
        }
    }

    let cache = cache.clone();
    tokio::task::spawn_blocking(move || {
        let mut guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((cached_dir, cached)) = &guard.entry {
            if cached_dir == &model_dir {
                return cached.clone();
            }
        }
        let custom_dir = if model_dir.is_empty() {
            None
        } else {
            Some(Path::new(&model_dir))
        };
        let initialized = LocalPpOcrEngine::new(custom_dir)
            .map(Arc::new)
            .map_err(|error| error.to_string());
        guard.entry = Some((model_dir, initialized.clone()));
        initialized
    })
    .await
    .map_err(|error| format!("OCR 引擎初始化任务异常: {error}"))?
}

/// 执行 OCR 并返回全文。
async fn execute_ocr(
    cache: &Arc<Mutex<OcrCache>>,
    image: image::RgbaImage,
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
        Ok(Ok(result)) => MediaJobOutcome::Ocr {
            text: result.full_text,
        },
        Ok(Err(error)) => MediaJobOutcome::Failed {
            message: error.to_string(),
        },
        Err(error) => MediaJobOutcome::Failed {
            message: format!("OCR 推理任务异常: {error}"),
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
    use std::sync::Mutex;

    /// 相同模型目录必须命中已有缓存，不同目录必须触发重建。
    #[tokio::test]
    async fn test_ocr_cache_key_changes_with_model_dir() {
        let cache = Arc::new(Mutex::new(OcrCache::default()));
        let mut config = Config::default();
        config.ocr.local.model_dir = "/tmp/nonexistent-model-a".to_string();
        let first = get_ocr_engine(&cache, &config).await;
        assert!(first.is_err(), "不存在的模型目录必须返回错误");
        let cached = cache.lock().expect("OCR 缓存锁可用").entry.clone();
        let (cached_dir, _) = cached.expect("错误结果也必须按目录缓存");
        assert_eq!(cached_dir, "/tmp/nonexistent-model-a");

        config.ocr.local.model_dir = "/tmp/nonexistent-model-b".to_string();
        let second = get_ocr_engine(&cache, &config).await;
        assert!(second.is_err());
        let cached = cache.lock().expect("OCR 缓存锁可用").entry.clone();
        let (cached_dir, _) = cached.expect("新目录必须写入新缓存");
        assert_eq!(cached_dir, "/tmp/nonexistent-model-b");
    }
}
