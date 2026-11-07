//! 截图任务结果回填与状态流转。

use super::super::actions::build_pin_item;
use super::SnipperJobKind;
use crate::ports::{JobId, MediaJobOutcome, OcrTextItem, QrCodeItem};
use crate::snipper::window::clipboard::SnipperClipboardAction;
use crate::snipper::window::ocr_editor::OcrTextEditorState;
use crate::snipper::window::redaction::{apply_redaction, RedactionPreview};
use crate::snipper::window::state::SnipperState;
use crate::snipper::window::types::SnipperOutcome;
use image::RgbaImage;
use snapin_annotation::action::AnnotationShape;
use snapin_annotation::shapes::{TranslationAction, TranslationShapeItem};
use std::path::PathBuf;

/// 处理后台任务结果，驱动截图会话状态流转。
pub fn handle_job_outcome(
    state: &mut SnipperState,
    job_id: JobId,
    kind: SnipperJobKind,
    outcome: MediaJobOutcome,
) {
    let Some(pending) = state.take_matching_job(kind, job_id) else {
        tracing::debug!("丢弃过期的截图媒体任务结果: {job_id:?}");
        return;
    };

    match outcome {
        MediaJobOutcome::Ocr { text, lines } => handle_ocr_outcome(state, kind, text, lines),
        MediaJobOutcome::QrCodes { codes } => handle_qr_outcome(state, codes),
        MediaJobOutcome::Translated { items } => handle_translation_outcome(state, items),
        MediaJobOutcome::Beautified { image } => {
            handle_beautified_outcome(state, kind, image, pending.pin_position);
        }
        MediaJobOutcome::Saved { path } => handle_saved_outcome(state, kind, path),
        MediaJobOutcome::SaveCancelled => {
            state.set_status_message(snapin_foundation::i18n::tr("已取消保存"));
        }
        MediaJobOutcome::Failed { message } => handle_job_failure(state, kind, message),
    }
}

/// 处理后台美化结果：复制任务写入剪贴板，贴图任务按提交时固化的落点创建窗口。
fn handle_beautified_outcome(
    state: &mut SnipperState,
    kind: SnipperJobKind,
    image: RgbaImage,
    pin_position: Option<(i32, i32)>,
) {
    match kind {
        SnipperJobKind::Copy => {
            state.submit_clipboard_image(image, SnipperClipboardAction::CopyImage);
        }
        SnipperJobKind::Pin => {
            let Some(position) = pin_position else {
                tracing::error!("贴图任务缺少落点信息，美化结果已丢弃");
                return;
            };
            match build_pin_item(image, position) {
                Some(item) => state.session.outcome = Some(SnipperOutcome::Pinned(item)),
                None => state.set_status_message(snapin_foundation::i18n::tr("创建贴图缓冲区失败")),
            }
        }
        _ => {
            tracing::error!("美化结果与任务类别不匹配，任务类型: {kind:?}");
        }
    }
}

/// 处理 OCR 结果：智能打码进入预览，复制模式写入剪贴板，编辑模式打开全文编辑框。
fn handle_ocr_outcome(
    state: &mut SnipperState,
    kind: SnipperJobKind,
    text: String,
    lines: Vec<OcrTextItem>,
) {
    match kind {
        SnipperJobKind::Redact => handle_redact_outcome(state, lines),
        SnipperJobKind::Ocr => handle_ocr_copy_outcome(state, text),
        SnipperJobKind::OcrEdit => handle_ocr_edit_outcome(state, text),
        _ => {}
    }
}

/// 复制模式：真实写入剪贴板成功后才结束截图会话。
fn handle_ocr_copy_outcome(state: &mut SnipperState, text: String) {
    let text = text.trim().to_string();
    if text.is_empty() {
        state.set_status_message(snapin_foundation::i18n::tr("未识别到文字"));
        return;
    }
    state.submit_clipboard_text(text.clone(), SnipperClipboardAction::CopyText(text));
}

/// 编辑模式：将 OCR 全文放入单个多行编辑框。
fn handle_ocr_edit_outcome(state: &mut SnipperState, text: String) {
    if text.trim().is_empty() {
        state.set_status_message(snapin_foundation::i18n::tr("未识别到文字"));
        return;
    }
    state.ocr.editor = Some(OcrTextEditorState::new(text));
    state.clear_status_message();
}

/// 将智能打码命中的文字区域立即写入标注历史并保留可切换预览。
fn handle_redact_outcome(state: &mut SnipperState, lines: Vec<OcrTextItem>) {
    if lines.is_empty() {
        state.clear_status_message();
        return;
    }
    let preview = RedactionPreview::from_ocr_lines(&lines);
    let applied = apply_redaction(state, &preview);
    state.ocr.redaction_preview = (applied > 0).then_some(preview);
    state.clear_status_message();
}

/// 处理二维码结果：空结果清空旧状态，多结果复制第一项并保留全部结果。
fn handle_qr_outcome(state: &mut SnipperState, codes: Vec<QrCodeItem>) {
    if codes.is_empty() {
        state.ocr.qr_result = None;
        state.set_status_message(snapin_foundation::i18n::tr("未识别到二维码"));
        return;
    }
    let count = codes.len();
    let first = codes[0].text.clone();
    let suffix = if first.starts_with("http://") || first.starts_with("https://") {
        snapin_foundation::i18n::tr("；Ctrl+Enter 打开链接")
    } else {
        ""
    };
    let success_message = snapin_foundation::trf!(
        "已复制二维码内容（共 {} 个）{}",
        "QR content copied ({} codes){}",
        count,
        suffix
    );
    state.ocr.qr_result = Some(codes);
    state.submit_clipboard_text(
        first,
        SnipperClipboardAction::TransientStatus {
            success_message,
            failure_label: "复制二维码内容",
        },
    );
}

/// 将有效原位翻译结果作为一条复合标注提交。
fn handle_translation_outcome(state: &mut SnipperState, items: Vec<TranslationShapeItem>) {
    if items.is_empty() {
        state.set_status_message(snapin_foundation::i18n::tr("未获得有效翻译结果"));
        return;
    }
    state
        .editor
        .annotations
        .push(AnnotationShape::Translation(TranslationAction::new(items)));
    state.editor.annotation.toolbar.active_tool = None;
    state.editor.annotation.toolbar.sub_rect = None;
    state.clear_status_message();
    tracing::info!("原位翻译完成，已成功压入标注动作栈");
}

/// 保存成功时区分普通截图保存与 OCR 结果导出。
fn handle_saved_outcome(state: &mut SnipperState, kind: SnipperJobKind, path: PathBuf) {
    if kind == SnipperJobKind::Export {
        state.set_status_message(snapin_foundation::trf!(
            "OCR 结果已导出至: {}",
            "OCR results exported to: {}",
            path.display()
        ));
    } else {
        state.session.outcome = Some(SnipperOutcome::Saved(path));
    }
}

/// 将后台任务失败转换为带任务类型的中文提示。
fn handle_job_failure(state: &mut SnipperState, kind: SnipperJobKind, message: String) {
    tracing::warn!("{}后台任务失败: {message}", kind.label());
    let message = snapin_foundation::i18n::localize_error(&message);
    state.set_status_message(snapin_foundation::trf!(
        "{}失败: {}",
        "{} failed: {}",
        kind.label(),
        message
    ));
}

#[cfg(test)]
mod tests;
