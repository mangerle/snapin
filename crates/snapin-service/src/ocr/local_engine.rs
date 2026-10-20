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
use super::dbnet::{extract_bounding_boxes, DbPostProcessParams};
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
use std::path::Path;
use std::sync::{Arc, Mutex};
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
    fn recognize(&self, image: &RgbaImage) -> Result<OcrResult, ServiceError> {
        let (image_width, image_height) = image.dimensions();
        if image_width == 0 || image_height == 0 {
            return Ok(OcrResult::default());
        }

        let started = Instant::now();
        let detection_input = preprocess_detection_image(image);
        if detection_input.chw.is_empty() {
            return Ok(OcrResult::default());
        }

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| ServiceError::InferenceError {
                stage: "ocr_session_lock".to_string(),
                reason: "OCR 推理会话锁已中毒".to_string(),
            })?;

        let det_tensor = Tensor::from_array((
            vec![
                1_i64,
                3,
                detection_input.height as i64,
                detection_input.width as i64,
            ],
            detection_input.chw,
        ))
        .map_err(|error| inference_error("det_input", error))?;

        let det_started = Instant::now();
        let det_outputs = sessions
            .det
            .run(ort::inputs![det_tensor])
            .map_err(|error| inference_error("det_run", error))?;
        let det_output = det_outputs
            .get("fetch_name_0")
            .ok_or_else(|| missing_output_error("det_run", "fetch_name_0"))?;
        let (det_shape, probability_map) = det_output
            .try_extract_tensor::<f32>()
            .map_err(|error| inference_error("det_output", error))?;

        if det_shape.len() != 4 {
            return Err(ServiceError::InferenceError {
                stage: "det_output".to_string(),
                reason: format!("检测模型输出维度异常: {:?}", det_shape),
            });
        }

        let map_height = det_shape[2] as usize;
        let map_width = det_shape[3] as usize;
        let boxes = extract_bounding_boxes(
            probability_map,
            map_width,
            map_height,
            image_width,
            image_height,
            &self.det_params,
        );
        drop(det_outputs);

        tracing::debug!(
            "OCR 检测完成，文本框数量: {}，耗时: {:.2}ms",
            boxes.len(),
            det_started.elapsed().as_secs_f64() * 1000.0
        );

        if boxes.is_empty() {
            return Ok(OcrResult::default());
        }

        let mut blocks: Vec<OcrTextBlock> = Vec::with_capacity(boxes.len());
        let rec_started = Instant::now();

        for detected in &boxes {
            let Some(cropped) = crop_box(image, detected.bbox) else {
                continue;
            };

            let cropped = if cropped.height() as f32 > cropped.width() as f32 * 1.5 {
                imageops::rotate90(&cropped)
            } else {
                cropped
            };

            let (chw, rec_width, rec_height) =
                preprocess_recognition_image(&cropped, &self.rec_preprocess);
            if chw.is_empty() {
                continue;
            }

            let rec_tensor =
                Tensor::from_array((vec![1_i64, 3, rec_height as i64, rec_width as i64], chw))
                    .map_err(|error| inference_error("rec_input", error))?;

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
            let (text, confidence) =
                ctc_greedy_decode(logits, sequence_len, class_count, &self.dict);

            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }

            let bg_color = estimate_dominant_background_color(image, detected.bbox);
            let text_color = get_contrasting_text_color(bg_color);
            let mut block = OcrTextBlock::new(text, detected.bbox, detected.polygon, confidence);
            block.bg_color = bg_color;
            block.text_color = text_color;
            blocks.push(block);
        }

        tracing::debug!(
            "OCR 识别完成，有效文本块: {}，耗时: {:.2}ms，总耗时: {:.2}ms",
            blocks.len(),
            rec_started.elapsed().as_secs_f64() * 1000.0,
            started.elapsed().as_secs_f64() * 1000.0
        );

        Ok(OcrResult::from_blocks(blocks))
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
fn crop_box(image: &RgbaImage, bbox: (f32, f32, f32, f32)) -> Option<RgbaImage> {
    let image_width = image.width();
    let image_height = image.height();
    let x0 = bbox.0.floor().clamp(0.0, image_width as f32) as u32;
    let y0 = bbox.1.floor().clamp(0.0, image_height as f32) as u32;
    let x1 = (bbox.0 + bbox.2).ceil().clamp(0.0, image_width as f32) as u32;
    let y1 = (bbox.1 + bbox.3).ceil().clamp(0.0, image_height as f32) as u32;
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
