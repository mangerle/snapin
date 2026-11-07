//! OCR 与二维码识别结果等表现层共享数据类型。

use snapin_foundation::{Rect, RectI};

/// OCR 识别出的单个文本行，供文字识别编辑框和智能打码等表现层功能使用。
#[derive(Debug, Clone, PartialEq)]
pub struct OcrTextItem {
    /// 识别出的文本内容
    pub text: String,
    /// 文本行在截图选区坐标系中的包围盒
    pub bounds: Rect,
}

/// 二维码解码结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrCodeItem {
    /// 二维码内容
    pub text: String,
    /// 二维码在截图选区坐标系中的包围盒
    pub bounds: RectI,
}

/// OCR 文本导出格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrExportFormat {
    /// Markdown 文本
    Markdown,
    /// JSON 字符串数组
    Json,
}
