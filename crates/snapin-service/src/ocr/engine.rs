//! OCR 统一抽象特型与执行契约。

use crate::error::ServiceError;
use crate::ocr::types::OcrResult;
use image::RgbaImage;

/// 统一 OCR 引擎接口。
///
/// 推理是 CPU 密集型同步操作，调用方必须在阻塞线程池中执行，禁止直接放在异步运行时工作线程。
pub trait OcrEngine: Send + Sync {
    /// 对传入的原图或选区裁剪图像执行文字检测与文本识别。
    ///
    /// # Errors
    ///
    /// 当 ONNX Runtime 未安装、模型文件缺失、推理会话执行失败或模型输出格式异常时返回
    /// [`ServiceError::InferenceError`] 或 [`ServiceError::ModelNotFound`]。
    fn recognize(&self, image: &RgbaImage) -> Result<OcrResult, ServiceError>;
}
