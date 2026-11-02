//! 基于 PP-OCRv6 的本地离线 OCR 推理引擎实现。
//!
//! 采用“检测 → 按框裁剪 → 逐行识别 → CTC 解码”的两阶段管线：
//! 1. DBNet 检测模型输出文本行概率图；
//! 2. 后处理得到原图坐标下的文本框；
//! 3. 识别模型对每个文本框裁剪图执行 CRNN/CTC 解码；
//! 4. 汇总文本块、背景色与高对比度前景色，供复制与原位翻译使用。
//!
//! 推理会话在引擎生命周期内复用，调用方应在阻塞线程池中执行 [`OcrEngine::recognize`]。

use super::config::{load_character_dict, LocalOcrPaths};
use super::ctc::ctc_greedy_decode;
use super::dbnet::{extract_bounding_boxes, DbPostProcessParams, DetectedBox, DetectionShape};
use super::engine::OcrEngine;
use super::image_ops::{
    estimate_dominant_background_color, get_contrasting_text_color, preprocess_detection_image,
    preprocess_recognition_image, PreprocessParams,
};
use super::onnx_runtime;
use super::types::{OcrResult, OcrTextBlock};
use crate::error::ServiceError;
use image::{imageops, RgbaImage};
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;
use snapin_foundation::Rect;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

/// 本地 PP-OCR 推理引擎（内部共享会话，可安全克隆后跨线程传递）
#[derive(Clone)]
pub struct LocalPpOcrEngine {
    inner: Arc<EngineInner>,
}

struct EngineInner {
    paths: LocalOcrPaths,
    dict: Vec<String>,
    sessions: Mutex<OcrSessions>,
    rec_preprocess: PreprocessParams,
    det_params: DbPostProcessParams,
}

struct OcrSessions {
    det: Session,
    rec: Session,
}

impl LocalPpOcrEngine {
    /// 自动根据 XDG 与系统路径探测并初始化本地引擎。
    ///
    /// # Errors
    ///
    /// 当模型文件缺失、字符字典解析失败或 ONNX Runtime 动态库不可用时返回领域错误。
    pub fn new(custom_dir: Option<&Path>) -> Result<Self, ServiceError> {
        let paths = LocalOcrPaths::resolve_model_paths(custom_dir)?;
        let dict = load_character_dict(&paths.rec_config)?;
        let env = onnx_runtime::environment()?;
        let threads = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(2)
            .clamp(1, 4);

        let det = build_session(&env, &paths.det_model, threads, "det_session")?;
        let rec = build_session(&env, &paths.rec_model, threads, "rec_session")?;

        tracing::info!(
            "已成功初始化本地 PP-OCR 引擎，检测模型: {:?}，识别模型: {:?}，字典规模: {} 字符",
            paths.det_model,
            paths.rec_model,
            dict.len()
        );

        Ok(Self {
            inner: Arc::new(EngineInner {
                paths,
                dict,
                sessions: Mutex::new(OcrSessions { det, rec }),
                rec_preprocess: PreprocessParams::default(),
                det_params: DbPostProcessParams::default(),
            }),
        })
    }

    /// 获取模型路径清单
    pub fn paths(&self) -> &LocalOcrPaths {
        &self.inner.paths
    }

    /// 获取字典总字符数
    pub fn dict_len(&self) -> usize {
        self.inner.dict.len()
    }
}

impl OcrEngine for LocalPpOcrEngine {
    fn recognize(&self, image: &RgbaImage) -> Result<OcrResult, ServiceError> {
        self.inner.recognize(image)
    }
}

impl EngineInner {
    /// 获取推理会话锁；锁中毒表示此前的推理线程发生过 panic。
    fn lock_sessions(&self) -> MutexGuard<'_, OcrSessions> {
        self.sessions.lock().unwrap_or_else(|poisoned| {
            tracing::debug!("OCR 推理会话锁已中毒，恢复内部状态后继续推理");
            poisoned.into_inner()
        })
    }

    fn recognize(&self, image: &RgbaImage) -> Result<OcrResult, ServiceError> {
        if image.width() == 0 || image.height() == 0 {
            return Ok(OcrResult::default());
        }

        let started = Instant::now();
        let det_started = Instant::now();
        let boxes = self.detect_text_boxes(image)?;
        tracing::debug!(
            "OCR 检测完成，文本框数量: {}，耗时: {:.2}ms",
            boxes.len(),
            det_started.elapsed().as_secs_f64() * 1000.0
        );
        if boxes.is_empty() {
            return Ok(OcrResult::default());
        }

        let rec_started = Instant::now();
        let (blocks, skipped_lines) = self.recognize_boxes(image, &boxes);
        if skipped_lines > 0 {
            tracing::warn!(
                "本次识别跳过 {} / {} 行失败文本，返回 {} 个有效文本块",
                skipped_lines,
                boxes.len(),
                blocks.len()
            );
        }
        tracing::debug!(
            "OCR 识别完成，有效文本块: {}，耗时: {:.2}ms，总耗时: {:.2}ms",
            blocks.len(),
            rec_started.elapsed().as_secs_f64() * 1000.0,
            started.elapsed().as_secs_f64() * 1000.0
        );
        Ok(OcrResult::from_blocks(blocks))
    }

    /// 执行文本检测模型并返回原图坐标系下的文本框。
    fn detect_text_boxes(&self, image: &RgbaImage) -> Result<Vec<DetectedBox>, ServiceError> {
        let detection_input = preprocess_detection_image(image);
        if detection_input.chw.is_empty() {
            return Ok(Vec::new());
        }
        let (det_dims, probability_map) = self.run_detection(
            detection_input.chw,
            detection_input.height as usize,
            detection_input.width as usize,
        )?;
        let map_height = det_dims[2];
        let map_width = det_dims[3];
        Ok(extract_bounding_boxes(
            &probability_map,
            DetectionShape::new(map_width, map_height, image.width(), image.height()),
            &self.det_params,
        ))
    }

    /// 构造检测输入张量、执行推理并复制输出数据。
    fn run_detection(
        &self,
        chw: Vec<f32>,
        height: usize,
        width: usize,
    ) -> Result<(Vec<usize>, Vec<f32>), ServiceError> {
        let tensor = Tensor::from_array((vec![1_i64, 3, height as i64, width as i64], chw))
            .map_err(|error| inference_error("det_input", error))?;
        let mut sessions = self.lock_sessions();
        let outputs = sessions
            .det
            .run(ort::inputs![tensor])
            .map_err(|error| inference_error("det_run", error))?;
        let output = outputs
            .get("fetch_name_0")
            .ok_or_else(|| missing_output_error("det_run", "fetch_name_0"))?;
        let (shape, probability_map) = output
            .try_extract_tensor::<f32>()
            .map_err(|error| inference_error("det_output", error))?;
        if shape.len() != 4 {
            return Err(ServiceError::InferenceError {
                stage: "det_output".to_string(),
                reason: format!("检测模型输出维度异常: {:?}", shape),
            });
        }
        let dimensions = shape
            .iter()
            .map(|&value| {
                usize::try_from(value).map_err(|_| ServiceError::InferenceError {
                    stage: "det_output".to_string(),
                    reason: format!("检测模型输出维度包含非法值: {value}"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((dimensions, probability_map.to_vec()))
    }

    /// 逐框裁剪识别；单行失败只跳过该框并返回失败数量。
    fn recognize_boxes(
        &self,
        image: &RgbaImage,
        boxes: &[DetectedBox],
    ) -> (Vec<OcrTextBlock>, usize) {
        let mut blocks = Vec::with_capacity(boxes.len());
        let mut skipped_lines = 0usize;
        for (index, detected) in boxes.iter().enumerate() {
            match self.recognize_detected_box(image, index, detected) {
                Ok(Some(block)) => blocks.push(block),
                Ok(None) => {}
                Err(error) => {
                    skipped_lines += 1;
                    tracing::warn!(
                        "单行文本识别失败，已跳过该行，文本框序号: {}，区域: {:?}，原因: {}",
                        index,
                        detected.bbox,
                        error
                    );
                }
            }
        }
        (blocks, skipped_lines)
    }

    /// 裁剪、方向校正并识别一个检测框，同时计算背景和前景色。
    fn recognize_detected_box(
        &self,
        image: &RgbaImage,
        _index: usize,
        detected: &DetectedBox,
    ) -> Result<Option<OcrTextBlock>, ServiceError> {
        let Some(cropped) = crop_box(image, detected.bbox) else {
            return Ok(None);
        };
        let cropped = if cropped.height() as f32 > cropped.width() as f32 * 1.5 {
            imageops::rotate90(&cropped)
        } else {
            cropped
        };
        let Some((text, confidence)) = self.recognize_text_line(&cropped)? else {
            return Ok(None);
        };
        let bg_color = estimate_dominant_background_color(image, detected.bbox);
        let text_color = get_contrasting_text_color(bg_color);
        let mut block = OcrTextBlock::new(text, detected.bbox, detected.polygon, confidence);
        block.bg_color = bg_color;
        block.text_color = text_color;
        Ok(Some(block))
    }
}

/// 对单个文本框裁剪图执行一次识别推理，返回去空白后的文本与平均置信度。
///
/// 返回 `Ok(None)` 表示预处理后没有可用像素或解码结果为空，属于可正常跳过的
/// 情况；返回 `Err` 表示本次推理在输入构造、执行或输出解析环节失败，由调用方
/// 决定是否降级。
///
/// # Errors
///
/// 当输入张量构造失败、ONNX Runtime 推理失败、输出名称缺失、输出张量解析失败，
/// 或输出维度不是 `[1, 序列长度, 类别数]` 时返回 [`ServiceError::InferenceError`]。
impl EngineInner {
    /// 对单个文本框裁剪图执行一次识别推理，返回去空白后的文本与平均置信度。
    ///
    /// 预处理与张量构造在锁外完成，锁只覆盖单次识别推理与 CTC 解码；
    /// 这样逐行识别不会在整张图的生命周期内独占会话，其他 OCR 任务可以穿插执行。
    fn recognize_text_line(
        &self,
        cropped: &RgbaImage,
    ) -> Result<Option<(String, f32)>, ServiceError> {
        let (chw, rec_width, rec_height) =
            preprocess_recognition_image(cropped, &self.rec_preprocess);
        if chw.is_empty() {
            return Ok(None);
        }

        let rec_tensor =
            Tensor::from_array((vec![1_i64, 3, rec_height as i64, rec_width as i64], chw))
                .map_err(|error| inference_error("rec_input", error))?;

        let mut sessions = self.lock_sessions();
        let rec_outputs = sessions
            .rec
            .run(ort::inputs![rec_tensor])
            .map_err(|error| inference_error("rec_run", error))?;
        let rec_output = rec_outputs
            .get("fetch_name_0")
            .ok_or_else(|| missing_output_error("rec_run", "fetch_name_0"))?;
        let (rec_shape, logits) = rec_output
            .try_extract_tensor::<f32>()
            .map_err(|error| inference_error("rec_output", error))?;

        if rec_shape.len() != 3 {
            return Err(ServiceError::InferenceError {
                stage: "rec_output".to_string(),
                reason: format!("识别模型输出维度异常: {:?}", rec_shape),
            });
        }

        let sequence_len = rec_shape[1] as usize;
        let class_count = rec_shape[2] as usize;
        let (text, confidence) = ctc_greedy_decode(logits, sequence_len, class_count, &self.dict);
        let text = text.trim().to_string();
        if text.is_empty() {
            return Ok(None);
        }

        Ok(Some((text, confidence)))
    }
}

/// 构建单个 ONNX 推理会话。
fn build_session(
    env: &Environment,
    model_path: &Path,
    threads: usize,
    stage: &str,
) -> Result<Session, ServiceError> {
    if !model_path.exists() {
        return Err(ServiceError::ModelNotFound {
            path: model_path.to_string_lossy().to_string(),
        });
    }

    Session::builder(env)
        .map_err(|error| inference_error(stage, error))?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|error| inference_error(stage, error))?
        .with_intra_threads(threads)
        .map_err(|error| inference_error(stage, error))?
        .commit_from_file(model_path)
        .map_err(|error| inference_error(stage, error))
}

/// 从原图中裁剪文本框区域，并做边界裁剪保护。
fn crop_box(image: &RgbaImage, bbox: Rect) -> Option<RgbaImage> {
    let image_width = image.width();
    let image_height = image.height();
    let x0 = bbox.x.floor().clamp(0.0, image_width as f32) as u32;
    let y0 = bbox.y.floor().clamp(0.0, image_height as f32) as u32;
    let x1 = (bbox.x + bbox.width).ceil().clamp(0.0, image_width as f32) as u32;
    let y1 = (bbox.y + bbox.height)
        .ceil()
        .clamp(0.0, image_height as f32) as u32;
    let width = x1.saturating_sub(x0);
    let height = y1.saturating_sub(y0);
    if width < 2 || height < 2 {
        return None;
    }

    Some(imageops::crop_imm(image, x0, y0, width, height).to_image())
}

fn inference_error(stage: &str, error: impl std::fmt::Display) -> ServiceError {
    ServiceError::InferenceError {
        stage: stage.to_string(),
        reason: error.to_string(),
    }
}

fn missing_output_error(stage: &str, output_name: &str) -> ServiceError {
    ServiceError::InferenceError {
        stage: stage.to_string(),
        reason: format!("模型未返回预期输出: {output_name}"),
    }
}
