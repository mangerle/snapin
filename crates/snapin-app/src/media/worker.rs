//! 媒体工作线程：任务循环、OCR/翻译服务缓存与任务分派。

use super::save::{execute_save_paster, execute_save_screenshot, SavePasterRequest};
use super::translate::{build_translation_items, to_service_provider, TranslationPlacement};
use super::WorkerMessage;
use snapin_core::config::Config;
use snapin_service::ocr::{LocalPpOcrEngine, OcrEngine};
use snapin_service::{create_translator, ServiceTranslateProvider, Translator, TranslatorOptions};
use snapin_ui::{MediaJob, MediaJobOutcome};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

/// 工作线程主循环。
pub(super) fn worker_loop(receiver: Receiver<WorkerMessage>, stopping: Arc<AtomicBool>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    if let Err(error) = &runtime {
        tracing::error!("创建媒体任务异步运行时失败，翻译与保存对话框不可用: {error}");
    }

    let mut state = WorkerState::default();
    while let Ok(message) = receiver.recv() {
        if stopping.load(Ordering::Acquire) {
            break;
        }
        match message {
            WorkerMessage::Stop => break,
            WorkerMessage::Job(item) => {
                if item.cancel.load(Ordering::Acquire) {
                    continue;
                }
                let outcome = match runtime.as_ref() {
                    Ok(runtime) => execute_job(&mut state, runtime, item.job, &item.config),
                    Err(error) => MediaJobOutcome::Failed {
                        message: format!("媒体任务异步运行时不可用: {error}"),
                    },
                };
                if !item.cancel.load(Ordering::Acquire) {
                    (item.sink)(outcome);
                }
            }
        }
    }
}

/// 工作线程内复用的 OCR 引擎与翻译器缓存。
#[derive(Default)]
struct WorkerState {
    ocr: Option<(String, Result<Arc<LocalPpOcrEngine>, String>)>,
    translator: Option<(TranslatorKey, Arc<dyn Translator>)>,
}

/// 翻译器缓存键：服务商与全部凭据任一变化都必须重建实例。
#[derive(Debug, Clone, PartialEq, Eq)]
struct TranslatorKey {
    provider: ServiceTranslateProvider,
    baidu_app_id: String,
    baidu_secret_key: String,
    youdao_app_key: String,
    youdao_app_secret: String,
    aliyun_access_key_id: String,
    aliyun_access_key_secret: String,
}

impl WorkerState {
    /// 按配置获取或初始化 OCR 引擎。
    ///
    /// 初始化失败同样按模型目录缓存，避免每次任务重复扫描同一批缺失文件；
    /// 用户修改模型目录后会因键变化重新尝试。
    fn ocr_engine(&mut self, config: &Config) -> Result<Arc<LocalPpOcrEngine>, String> {
        let model_dir = config.ocr.local.model_dir.trim().to_string();
        if let Some((cached_dir, cached)) = &self.ocr {
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
        self.ocr = Some((model_dir, initialized.clone()));
        initialized
    }

    /// 按配置获取或初始化翻译器，复用底层 HTTP 连接池。
    fn translator(&mut self, config: &Config) -> Arc<dyn Translator> {
        let key = TranslatorKey {
            provider: to_service_provider(config.translate.provider),
            baidu_app_id: config.translate.api_keys.baidu_app_id.clone(),
            baidu_secret_key: config.translate.api_keys.baidu_secret_key.clone(),
            youdao_app_key: config.translate.api_keys.youdao_app_key.clone(),
            youdao_app_secret: config.translate.api_keys.youdao_app_secret.clone(),
            aliyun_access_key_id: config.translate.api_keys.aliyun_access_key_id.clone(),
            aliyun_access_key_secret: config.translate.api_keys.aliyun_access_key_secret.clone(),
        };
        if let Some((cached_key, translator)) = &self.translator {
            if cached_key == &key {
                return translator.clone();
            }
        }

        let translator = create_translator(&TranslatorOptions {
            provider: key.provider,
            baidu_app_id: key.baidu_app_id.clone(),
            baidu_secret_key: key.baidu_secret_key.clone(),
            youdao_app_key: key.youdao_app_key.clone(),
            youdao_app_secret: key.youdao_app_secret.clone(),
            aliyun_access_key_id: key.aliyun_access_key_id.clone(),
            aliyun_access_key_secret: key.aliyun_access_key_secret.clone(),
        });
        self.translator = Some((key, translator.clone()));
        translator
    }
}

/// 执行一条媒体任务。
fn execute_job(
    state: &mut WorkerState,
    runtime: &tokio::runtime::Runtime,
    job: MediaJob,
    config: &Config,
) -> MediaJobOutcome {
    match job {
        MediaJob::Ocr { image } => execute_ocr(state, &image, config),
        MediaJob::Translate {
            image,
            origin,
            cover_mode,
        } => execute_translate(
            state,
            runtime,
            TranslateRequest {
                image,
                placement: TranslationPlacement { origin, cover_mode },
            },
            config,
        ),
        MediaJob::SaveScreenshot { image } => execute_save_screenshot(&image, config, runtime),
        MediaJob::SavePaster {
            image,
            as_dialog,
            open_folder_after,
        } => execute_save_paster(
            SavePasterRequest {
                image,
                as_dialog,
                open_folder_after,
            },
            config,
            runtime,
        ),
    }
}

/// 一次翻译请求所需的图像与放置参数。
struct TranslateRequest {
    image: image::RgbaImage,
    placement: TranslationPlacement,
}

/// 执行 OCR 并返回全文。
fn execute_ocr(
    state: &mut WorkerState,
    image: &image::RgbaImage,
    config: &Config,
) -> MediaJobOutcome {
    let engine = match state.ocr_engine(config) {
        Ok(engine) => engine,
        Err(message) => return MediaJobOutcome::Failed { message },
    };
    match engine.recognize(image) {
        Ok(result) => MediaJobOutcome::Ocr {
            text: result.full_text,
        },
        Err(error) => MediaJobOutcome::Failed {
            message: error.to_string(),
        },
    }
}

/// 执行 OCR、逐段翻译并生成原位翻译图元。
fn execute_translate(
    state: &mut WorkerState,
    runtime: &tokio::runtime::Runtime,
    request: TranslateRequest,
    config: &Config,
) -> MediaJobOutcome {
    let TranslateRequest { image, placement } = request;
    let engine = match state.ocr_engine(config) {
        Ok(engine) => engine,
        Err(message) => return MediaJobOutcome::Failed { message },
    };
    let ocr_result = match engine.recognize(&image) {
        Ok(result) => result,
        Err(error) => {
            return MediaJobOutcome::Failed {
                message: error.to_string(),
            }
        }
    };
    if ocr_result.blocks.is_empty() {
        return MediaJobOutcome::Translated { items: Vec::new() };
    }

    let mut texts = Vec::with_capacity(ocr_result.blocks.len());
    for block in &ocr_result.blocks {
        texts.push(block.text.clone());
    }
    let translator = state.translator(config);
    let translated = runtime.block_on(translator.translate_batch(
        &texts,
        "auto",
        config.translate.target_language.as_str(),
    ));
    let translated = match translated {
        Ok(translated) => translated,
        Err(error) => {
            return MediaJobOutcome::Failed {
                message: error.to_string(),
            }
        }
    };

    MediaJobOutcome::Translated {
        items: build_translation_items(&image, &ocr_result.blocks, &translated, placement),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 相同模型目录必须命中已有缓存，不同目录必须触发重建。
    #[test]
    fn test_ocr_cache_key_changes_with_model_dir() {
        let mut state = WorkerState::default();
        let mut config = Config::default();
        config.ocr.local.model_dir = "/tmp/nonexistent-model-a".to_string();
        let first = state.ocr_engine(&config);
        assert!(first.is_err(), "不存在的模型目录必须返回错误");
        let cached = state.ocr.clone().expect("错误结果也必须按目录缓存");
        assert_eq!(cached.0, "/tmp/nonexistent-model-a");

        config.ocr.local.model_dir = "/tmp/nonexistent-model-b".to_string();
        let second = state.ocr_engine(&config);
        assert!(second.is_err());
        let cached = state.ocr.clone().expect("新目录必须写入新缓存");
        assert_eq!(cached.0, "/tmp/nonexistent-model-b");
    }
}
