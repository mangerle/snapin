//! 字符检测与文字识别子系统。

pub mod config;
pub mod ctc;
pub mod dbnet;
pub mod engine;
pub mod image_ops;
pub mod local_engine;
pub mod onnx_runtime;
pub mod types;

pub use config::{load_character_dict, LocalOcrPaths, ModelInferenceConfig};
pub use ctc::ctc_greedy_decode;
pub use dbnet::{extract_bounding_boxes, DbPostProcessParams, DetectedBox};
pub use engine::OcrEngine;
pub use image_ops::{
    estimate_dominant_background_color, get_contrasting_text_color, preprocess_detection_image,
    preprocess_recognition_image, DetectionInput, PreprocessParams,
};
pub use local_engine::LocalPpOcrEngine;
pub use onnx_runtime::is_runtime_available;
pub use types::{OcrResult, OcrTextBlock};
