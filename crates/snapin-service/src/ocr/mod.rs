//! 字符检测与文字识别子系统。

mod config;
mod ctc;
mod dbnet;
mod engine;
mod image_ops;
mod local_engine;
mod onnx_runtime;
mod types;

pub use config::{load_character_dict, LocalOcrPaths, ModelInferenceConfig};
pub use ctc::ctc_greedy_decode;
pub use dbnet::{extract_bounding_boxes, DbPostProcessParams, DetectedBox, DetectionShape};
pub use engine::OcrEngine;
pub use image_ops::{
    estimate_dominant_background_color, get_contrasting_text_color, preprocess_detection_image,
    preprocess_recognition_image, DetectionInput, PreprocessParams,
};
pub use local_engine::LocalPpOcrEngine;
pub use onnx_runtime::is_runtime_available;
pub use types::{OcrResult, OcrTextBlock};
