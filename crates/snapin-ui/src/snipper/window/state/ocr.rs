//! 截图智能打码、二维码与文字识别交互状态。

use super::super::ocr_editor::OcrTextEditorState;
use super::super::ocr_menu::OcrContextMenuState;
use super::super::redaction::RedactionPreview;
use crate::ports::QrCodeItem;

/// OCR、二维码与智能打码结果状态。
#[derive(Default)]
pub(crate) struct SnipperOcr {
    /// 智能打码可点击文字区域。
    pub(crate) redaction_preview: Option<RedactionPreview>,
    /// 最近一次二维码识别结果。
    pub(crate) qr_result: Option<Vec<QrCodeItem>>,
    /// 文字识别主按钮的右键菜单。
    pub(crate) menu: Option<OcrContextMenuState>,
    /// 包含完整 OCR 文本的多行编辑框。
    pub(crate) editor: Option<OcrTextEditorState>,
}
