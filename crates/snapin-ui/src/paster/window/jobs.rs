//! 贴图窗口整图 OCR 与翻译后台任务执行器。
//!
//! 贴图工作区运行在 winit 事件循环线程上，任何 ONNX 推理或 HTTP 翻译请求都会
//! 直接冻结界面，因此这里把任务投递到独立后台线程执行，结果再通过
//! `EventLoopProxy` 以用户事件回传 UI 线程。
//!
//! 设计权衡：
//! - 队列使用容量固定的同步通道，天然形成背压；提交方采用快速失败策略，
//!   避免 UI 线程因等待空闲槽位而卡顿；
//! - OCR 引擎与翻译器在首次使用时惰性初始化并复用，避免每次任务重复加载模型
//!   与建立连接池；配置热重载时主动失效缓存，使模型目录与服务商修改立即生效；
//! - 工作线程由执行器持有并在 `shutdown` 时 join，杜绝脱缰线程。

use crate::error::UiError;
use crate::paster::workspace::proxy::PasterAppEvent;
use crate::snipper::window::jobs::{build_translation_items, translator_options};
use image::RgbaImage;
use snapin_core::annotation::shapes::TranslationShapeItem;
use snapin_core::config::Config;
use snapin_service::ocr::{LocalPpOcrEngine, OcrEngine};
use snapin_service::{create_translator, Translator};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use tiny_skia::Pixmap;
use winit::event_loop::EventLoopProxy;

/// 后台任务队列容量：允许一个任务执行、一个任务排队，超出后提交方立即失败。
const JOB_QUEUE_CAPACITY: usize = 2;

/// 后台任务结果回传通道抽象。
///
/// 抽出特型是为了让执行器脱离真实 `EventLoopProxy` 也能做单元测试；
/// 生产环境由 [`EventLoopSink`] 封装 winit 用户事件。
pub(crate) trait PasterJobSink: Send {
    /// 回传整图文字识别结果
    fn ocr_finished(&self, result: Result<String, String>);
    /// 回传整图翻译结果
    fn translate_finished(&self, result: Result<Vec<TranslationShapeItem>, String>);
}

/// 通过 winit 事件循环代理回传结果的实现。
struct EventLoopSink {
    proxy: EventLoopProxy<PasterAppEvent>,
}

impl PasterJobSink for EventLoopSink {
    fn ocr_finished(&self, result: Result<String, String>) {
        if self
            .proxy
            .send_event(PasterAppEvent::OcrFinished(result))
            .is_err()
        {
            tracing::warn!("贴图工作区已关闭，文字识别结果未能回传");
        }
    }

    fn translate_finished(&self, result: Result<Vec<TranslationShapeItem>, String>) {
        if self
            .proxy
            .send_event(PasterAppEvent::TranslateFinished(result))
            .is_err()
        {
            tracing::warn!("贴图工作区已关闭，翻译结果未能回传");
        }
    }
}

/// 投递到后台线程的贴图任务。
enum PasterJob {
    /// 整图文字识别
    Ocr {
        image: RgbaImage,
        config: Arc<Config>,
    },
    /// 整图 OCR 后原位翻译
    Translate {
        image: RgbaImage,
        config: Arc<Config>,
    },
    /// 停止工作线程
    Stop,
}

/// 工作线程内复用的 OCR 引擎与翻译器缓存。
///
/// 缓存由执行器与工作线程共享，配置热重载时由 [`PasterJobExecutor::update_config`]
/// 统一清空；正在执行的任务已持有旧实例，会自然完成，后续任务再按新配置重建。
struct JobServices {
    ocr_engine: RwLock<Option<Result<Arc<LocalPpOcrEngine>, String>>>,
    translator: RwLock<Option<Arc<dyn Translator>>>,
}

impl JobServices {
    fn new() -> Self {
        Self {
            ocr_engine: RwLock::new(None),
            translator: RwLock::new(None),
        }
    }

    /// 按配置惰性初始化本地 OCR 引擎；初始化失败同样缓存，避免后续任务反复加载。
    fn ocr_engine(&self, config: &Config) -> Result<Arc<LocalPpOcrEngine>, String> {
        if let Some(cached) = self
            .ocr_engine
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
        {
            return cached.clone();
        }

        let initialized = {
            let model_dir = config.ocr.local.model_dir.trim();
            let custom_dir = if model_dir.is_empty() {
                None
            } else {
                Some(Path::new(model_dir))
            };
            LocalPpOcrEngine::new(custom_dir)
                .map(Arc::new)
                .map_err(|error| error.to_string())
        };
        *self
            .ocr_engine
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(initialized.clone());
        initialized
    }

    /// 按配置惰性创建翻译器，复用底层 HTTP 连接池。
    fn translator(&self, config: &Config) -> Arc<dyn Translator> {
        if let Some(cached) = self
            .translator
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
        {
            return cached.clone();
        }

        let translator = create_translator(&translator_options(&config.translate));
        *self
            .translator
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(translator.clone());
        translator
    }

    /// 清空所有缓存，使下一次任务按最新配置重新初始化服务。
    fn invalidate(&self) {
        *self
            .ocr_engine
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .translator
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

/// 贴图工作区持有的后台任务执行器。
///
/// 所有方法均通过内部同步原语实现，可被多个贴图窗口状态共享；
/// 工作区退出时只需调用一次 [`Self::shutdown`]，重复调用安全。
pub(crate) struct PasterJobExecutor {
    sender: Mutex<Option<SyncSender<PasterJob>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    /// 是否有任务尚未回传结果，防止多个贴图窗口同时提交导致结果无法路由
    busy: AtomicBool,
    /// 是否已请求停机，用于拒绝新任务并让工作线程丢弃排队任务
    stopping: Arc<AtomicBool>,
    /// 供 OCR 任务读取的最新配置快照
    config: RwLock<Arc<Config>>,
    /// 与工作线程共享的 OCR/翻译服务缓存，配置热重载时可主动失效
    services: Arc<JobServices>,
}

impl PasterJobExecutor {
    /// 创建绑定 winit 事件循环代理的执行器并拉起命名工作线程。
    ///
    /// # Errors
    ///
    /// 工作线程派生失败时返回 [`UiError::Window`]。
    pub(crate) fn try_new(
        proxy: EventLoopProxy<PasterAppEvent>,
        config: Arc<Config>,
    ) -> Result<Self, UiError> {
        Self::with_sink(Box::new(EventLoopSink { proxy }), config)
    }

    /// 使用任意结果回传通道创建执行器，供单元测试注入桩实现。
    fn with_sink(sink: Box<dyn PasterJobSink>, config: Arc<Config>) -> Result<Self, UiError> {
        let (sender, receiver) = sync_channel(JOB_QUEUE_CAPACITY);
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stopping = stopping.clone();
        let services = Arc::new(JobServices::new());
        let worker_services = services.clone();
        let worker = thread::Builder::new()
            .name("snapin-paster-job".to_string())
            .spawn(move || worker_loop(receiver, sink, worker_stopping, worker_services))
            .map_err(|error| UiError::Window {
                message: format!("派生贴图后台任务线程失败: {error}"),
            })?;

        Ok(Self {
            sender: Mutex::new(Some(sender)),
            worker: Mutex::new(Some(worker)),
            busy: AtomicBool::new(false),
            stopping,
            config: RwLock::new(config),
            services,
        })
    }

    /// 提交整图文字识别任务；成功入队后由工作线程异步执行。
    ///
    /// # Errors
    ///
    /// 已有任务在执行、队列已满、执行器已停机或工作线程意外退出时返回中文错误。
    pub(crate) fn submit_ocr(&self, image: RgbaImage) -> Result<(), String> {
        let config = self.config_snapshot();
        self.submit(PasterJob::Ocr { image, config })
    }

    /// 提交整图翻译任务；`config` 提供目标语言与背景覆盖模式。
    ///
    /// # Errors
    ///
    /// 与 [`Self::submit_ocr`] 相同，任务被拒绝时不会占用执行器。
    pub(crate) fn submit_translate(
        &self,
        image: RgbaImage,
        config: Arc<Config>,
    ) -> Result<(), String> {
        self.submit(PasterJob::Translate { image, config })
    }

    /// 更新后续任务的配置快照，并失效已缓存的 OCR 引擎与翻译器。
    ///
    /// 正在执行的任务持有旧实例会自然完成；下一次任务将按新配置重新初始化，
    /// 使设置页修改模型目录或翻译服务商后无需重启守护进程即可生效。
    pub(crate) fn update_config(&self, config: Arc<Config>) {
        *self
            .config
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = config;
        self.services.invalidate();
        tracing::debug!("贴图后台 OCR 与翻译服务缓存已失效，将在下次任务时按新配置重建");
    }

    /// 标记当前任务已回传结果，允许提交下一个任务。
    pub(crate) fn complete(&self) {
        self.busy.store(false, Ordering::Release);
    }

    /// 请求工作线程停止并等待其退出。
    ///
    /// 先置停机标志再发送停止信号：工作线程若正在执行任务会先完成当前任务，
    /// 随后丢弃尚未开始的排队任务并退出；重复调用不会 panic。
    pub(crate) fn shutdown(&self) {
        self.stopping.store(true, Ordering::Release);

        let sender = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(sender) = sender {
            if sender.send(PasterJob::Stop).is_err() {
                tracing::debug!("贴图后台任务线程已退出，停止信号未送达");
            }
        }

        let worker = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(worker) = worker {
            if worker.join().is_err() {
                tracing::error!("贴图后台任务线程退出时发生 panic，可能残留未完成任务");
            }
        }
    }

    /// 读取当前配置快照。
    fn config_snapshot(&self) -> Arc<Config> {
        self.config
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// 统一的入队与忙碌状态管理。
    fn submit(&self, job: PasterJob) -> Result<(), String> {
        if self.stopping.load(Ordering::Acquire) {
            return Err("后台任务执行器已停止".to_string());
        }
        if self.busy.swap(true, Ordering::AcqRel) {
            return Err("已有文字识别或翻译任务正在执行".to_string());
        }

        let result = self.enqueue(job);
        if result.is_err() {
            self.busy.store(false, Ordering::Release);
        }
        result
    }

    /// 尝试把任务放入有界队列，队列满时快速失败而不是阻塞 UI 线程。
    fn enqueue(&self, job: PasterJob) -> Result<(), String> {
        let guard = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(sender) = guard.as_ref() else {
            return Err("后台任务执行器已停止".to_string());
        };
        match sender.try_send(job) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err("后台任务队列已满，请稍后重试".to_string()),
            Err(TrySendError::Disconnected(_)) => Err("后台任务线程已退出".to_string()),
        }
    }
}

impl Drop for PasterJobExecutor {
    /// 兜底回收工作线程，保证即使调用方遗漏显式停机也不会遗留脱缰线程。
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 后台工作线程主循环。
fn worker_loop(
    receiver: Receiver<PasterJob>,
    sink: Box<dyn PasterJobSink>,
    stopping: Arc<AtomicBool>,
    services: Arc<JobServices>,
) {
    // 截图侧复用守护进程运行时；贴图工作区可能运行在没有 Tokio 上下文的
    // 独立线程上，因此自建一个最小 current-thread 运行时承载翻译 HTTP 请求。
    let async_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();

    while let Ok(job) = receiver.recv() {
        if stopping.load(Ordering::Acquire) {
            break;
        }
        match job {
            PasterJob::Stop => break,
            PasterJob::Ocr { image, config } => {
                sink.ocr_finished(execute_ocr(&services, &image, &config));
            }
            PasterJob::Translate { image, config } => {
                let result = match async_runtime.as_ref() {
                    Ok(runtime) => execute_translate(&services, runtime, &image, &config),
                    Err(error) => Err(format!("创建翻译异步运行时失败: {error}")),
                };
                sink.translate_finished(result);
            }
        }
    }
}

/// 执行整图 OCR，返回合并后的全文。
fn execute_ocr(
    services: &JobServices,
    image: &RgbaImage,
    config: &Config,
) -> Result<String, String> {
    let engine = services.ocr_engine(config)?;
    engine
        .recognize(image)
        .map(|result| result.full_text)
        .map_err(|error| error.to_string())
}

/// 执行整图 OCR、逐段翻译并复用截图侧排版逻辑生成翻译图元。
fn execute_translate(
    services: &JobServices,
    runtime: &tokio::runtime::Runtime,
    image: &RgbaImage,
    config: &Config,
) -> Result<Vec<TranslationShapeItem>, String> {
    let engine = services.ocr_engine(config)?;
    let ocr_result = engine.recognize(image).map_err(|error| error.to_string())?;
    if ocr_result.blocks.is_empty() {
        return Ok(Vec::new());
    }

    let mut texts = Vec::with_capacity(ocr_result.blocks.len());
    for block in &ocr_result.blocks {
        texts.push(block.text.clone());
    }

    let translator = services.translator(config);
    let translated = runtime
        .block_on(translator.translate_batch(
            &texts,
            "auto",
            config.translate.target_language.as_str(),
        ))
        .map_err(|error| error.to_string())?;

    // 整图翻译以图像左上角为原点；背景覆盖模式沿用当前用户配置。
    Ok(build_translation_items(
        image,
        &ocr_result.blocks,
        &translated,
        (0, 0),
        config.translate.cover_mode,
    ))
}

/// 将 tiny-skia 位图转换为 OCR 引擎可直接消费的 RGBA 图像。
///
/// `Pixmap::data()` 为预乘 RGBA 字节序，与引擎入参的通道布局一致；
/// 这里仍显式校验尺寸与字节长度，避免上游异常导致越界读或尺寸错位。
pub(crate) fn pixmap_to_rgba_image(pixmap: &Pixmap) -> Result<RgbaImage, String> {
    rgba_from_premultiplied(pixmap.width(), pixmap.height(), pixmap.data())
}

/// 从预乘 RGBA 裸字节构造图像，并校验宽高乘积与缓冲区长度。
fn rgba_from_premultiplied(width: u32, height: u32, data: &[u8]) -> Result<RgbaImage, String> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| format!("图像尺寸溢出，宽度: {width}，高度: {height}"))?;
    if data.len() != expected {
        return Err(format!(
            "图像像素长度不匹配，期望 {expected} 字节，实际 {} 字节",
            data.len()
        ));
    }
    RgbaImage::from_raw(width, height, data.to_vec())
        .ok_or_else(|| format!("图像尺寸不合法，宽度: {width}，高度: {height}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use tiny_skia::Color;

    /// 不关心结果的空回传桩，用于验证执行器生命周期
    struct NullSink;

    impl PasterJobSink for NullSink {
        fn ocr_finished(&self, _result: Result<String, String>) {}
        fn translate_finished(&self, _result: Result<Vec<TranslationShapeItem>, String>) {}
    }

    fn test_executor() -> PasterJobExecutor {
        PasterJobExecutor::with_sink(Box::new(NullSink), Arc::new(Config::default()))
            .expect("测试后台执行器应成功创建")
    }

    /// 位图转换应保持宽高与字节长度一致
    #[test]
    fn 位图转换保持尺寸与像素长度() {
        let mut pixmap = Pixmap::new(4, 3).expect("创建测试位图失败");
        pixmap.fill(Color::from_rgba8(10, 20, 30, 255));

        let image = pixmap_to_rgba_image(&pixmap).expect("位图转换应成功");
        assert_eq!(image.dimensions(), (4, 3));
        assert_eq!(image.as_raw().len(), 4 * 3 * 4);
    }

    /// 缓冲区长度与宽高不匹配时必须返回错误而不是越界读取
    #[test]
    fn 像素长度不符时返回错误() {
        let error = rgba_from_premultiplied(2, 2, &[0u8; 15]).expect_err("长度不符必须失败");
        assert!(
            error.contains("长度不匹配"),
            "错误信息应说明长度不匹配，实际: {error}"
        );
    }

    /// 服务缓存失效后应清空旧配置的缓存条目，后续任务按新配置重建
    #[test]
    fn 失效服务缓存后条目被清空() {
        let services = JobServices::new();
        *services.ocr_engine.write().expect("获取 OCR 缓存写锁失败") =
            Some(Err("旧配置初始化失败".to_string()));
        *services.translator.write().expect("获取翻译器缓存写锁失败") = None;

        services.invalidate();

        assert!(
            services
                .ocr_engine
                .read()
                .expect("获取 OCR 缓存读锁失败")
                .is_none(),
            "失效后 OCR 缓存必须为空"
        );
        assert!(
            services
                .translator
                .read()
                .expect("获取翻译器缓存读锁失败")
                .is_none(),
            "失效后翻译器缓存必须为空"
        );
    }

    /// 执行器停机后提交任务应返回错误且不 panic
    #[test]
    fn 停机后提交任务返回错误() {
        let executor = test_executor();
        executor.shutdown();

        let image = RgbaImage::from_pixel(2, 2, Rgba([255, 255, 255, 255]));
        let ocr_error = executor
            .submit_ocr(image)
            .expect_err("停机后 OCR 提交必须失败");
        assert!(
            ocr_error.contains("已停止"),
            "错误信息应说明执行器已停止，实际: {ocr_error}"
        );

        let image = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
        let translate_error = executor
            .submit_translate(image, Arc::new(Config::default()))
            .expect_err("停机后翻译提交必须失败");
        assert!(
            translate_error.contains("已停止"),
            "错误信息应说明执行器已停止，实际: {translate_error}"
        );

        // 重复停机必须保持幂等
        executor.shutdown();
    }
}
