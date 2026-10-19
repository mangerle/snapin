//! OCR 统一抽象特型与执行契约。

use crate::error::ServiceError;
use crate::ocr::types::OcrResult;
use async_trait::async_trait;
use image::RgbaImage;

/// 统一 OCR 引擎接口
#[async_trait]
pub trait OcrEngine: Send + Sync {
    /// 对传入的原图或选区裁剪图像执行文字检测与文本识别
    async fn recognize(&self, image: &RgbaImage) -> Result<OcrResult, ServiceError>;
}
