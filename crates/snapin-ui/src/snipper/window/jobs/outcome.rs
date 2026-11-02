//! 截图任务结果回填与状态流转。

use super::SnipperJobKind;
use crate::ports::{JobId, MediaJobOutcome};
use crate::snipper::window::clipboard::SnipperClipboardAction;
use crate::snipper::window::ocr_panel::OcrPanelState;
use crate::snipper::window::redaction::{apply_redaction, RedactionPreview};
use crate::snipper::window::state::SnipperState;
use crate::snipper::window::types::SnipperOutcome;
use snapin_annotation::action::AnnotationShape;
use snapin_annotation::shapes::TranslationAction;

/// 处理后台任务结果，驱动截图会话状态流转。
pub fn handle_job_outcome(
    state: &mut SnipperState,
    job_id: JobId,
    kind: SnipperJobKind,
    outcome: MediaJobOutcome,
) {
    if !state.take_matching_job(kind, job_id) {
        tracing::debug!("丢弃过期的截图媒体任务结果: {job_id:?}");
        return;
    }

    match outcome {
        MediaJobOutcome::Ocr { text, lines } => {
            if kind == SnipperJobKind::Redact {
                if lines.is_empty() {
                    state.clear_status_message();
                    return;
                }
                let preview = RedactionPreview::from_ocr_lines(&lines);
                let applied = apply_redaction(state, &preview);
                state.ocr.redaction_preview = (applied > 0).then_some(preview);
                state.clear_status_message();
                return;
            }
            state.ocr.panel = Some(OcrPanelState::from_ocr_lines(&lines));
            let text = text.trim();
            if text.is_empty() {
                state.set_status_message(snapin_foundation::i18n::tr("未识别到文字"));
                return;
            }
            // 识别结果只写入剪贴板，不结束截图会话：用户常常要在同一张图上继续
            // 标注或再识别另一块区域。成功提示必须等真实剪贴板写入完成后展示。
            let success_message = snapin_foundation::trf!(
                "已复制识别文字（{} 字符）",
                "Recognized text copied ({} characters)",
                text.chars().count()
            );
            state.submit_clipboard_text(
                text.to_string(),
                SnipperClipboardAction::TransientStatus {
                    success_message,
                    failure_label: "复制识别文字",
                },
            );
        }
        MediaJobOutcome::QrCodes { codes } => {
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
        MediaJobOutcome::Translated { items } => {
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
        MediaJobOutcome::Saved { path } => {
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
        MediaJobOutcome::SaveCancelled => {
            state.set_status_message(snapin_foundation::i18n::tr("已取消保存"));
        }
        MediaJobOutcome::Failed { message } => {
            tracing::warn!("{}后台任务失败: {message}", kind.label());
            let message = snapin_foundation::i18n::localize_error(&message);
            state.set_status_message(snapin_foundation::trf!(
                "{}失败: {}",
                "{} failed: {}",
                kind.label(),
                message
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use crate::ports::MediaJob;
    use image::Rgba;
    use snapin_annotation::shapes::TranslationShapeItem;
    use snapin_config::config::CoverMode;
    use std::path::PathBuf;

    fn translation_item() -> TranslationShapeItem {
        TranslationShapeItem {
            rect: Rect::new(10.0, 10.0, 40.0, 20.0),
            original: "Hello".to_string(),
            translated: "你好".to_string(),
            bg_color: (255, 255, 255, 255).into(),
            text_color: (0, 0, 0, 255).into(),
            font_size: 14.0,
            cover_mode: CoverMode::SolidColor,
        }
    }

    /// 让状态接受一个测试任务 ID，模拟真实提交成功后的待处理态。
    fn accept_job(state: &mut SnipperState, kind: SnipperJobKind) -> JobId {
        let job_id = JobId::new(1);
        state.begin_pending_job(kind, job_id, Box::new(|| {}));
        job_id
    }

    #[test]
    fn test_handle_empty_ocr_outcome_keeps_session_open() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state, SnipperJobKind::Ocr);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Ocr,
            MediaJobOutcome::Ocr {
                text: String::new(),
                lines: Vec::new(),
            },
        );
        assert!(state.session.outcome.is_none());
        assert_eq!(
            state.session.status_message.as_deref(),
            Some("未识别到文字")
        );
    }

    /// 智能打码空结果不显示顶部提示，保持画布与工具栏原状。
    #[test]
    fn test_handle_empty_redact_outcome_does_not_show_status() {
        let mut state = SnipperState::new();
        state.set_status_message("智能打码执行中的旧提示");
        let job_id = accept_job(&mut state, SnipperJobKind::Redact);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Redact,
            MediaJobOutcome::Ocr {
                text: String::new(),
                lines: Vec::new(),
            },
        );

        assert!(state.ocr.redaction_preview.is_none());
        assert!(
            state.session.status_message.is_none(),
            "智能打码空结果不得保留顶部提示"
        );
    }

    #[test]
    fn test_handle_translate_outcome_pushes_annotation() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state, SnipperJobKind::Translate);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Translate,
            MediaJobOutcome::Translated {
                items: vec![translation_item()],
            },
        );
        assert!(state.editor.annotations.can_undo());
        assert!(state.session.status_message.is_none());
    }

    #[test]
    fn test_handle_saved_outcome_sets_result() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state, SnipperJobKind::Save);
        let path = PathBuf::from("/tmp/snapin_test.png");
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Save,
            MediaJobOutcome::Saved { path: path.clone() },
        );
        match state.session.outcome {
            Some(SnipperOutcome::Saved(saved_path)) => assert_eq!(saved_path, path),
            _ => panic!("保存成功必须返回 Saved 结果"),
        }
    }

    #[test]
    fn test_handle_save_cancelled_keeps_session_open() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state, SnipperJobKind::Save);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Save,
            MediaJobOutcome::SaveCancelled,
        );
        assert!(state.session.outcome.is_none(), "取消保存不得结束截图会话");
        assert_eq!(state.session.status_message.as_deref(), Some("已取消保存"));
    }

    #[test]
    fn test_handle_job_failure_sets_status() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state, SnipperJobKind::Translate);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Translate,
            MediaJobOutcome::Failed {
                message: "网络超时".to_string(),
            },
        );
        assert!(state
            .session
            .status_message
            .as_deref()
            .unwrap_or_default()
            .contains("网络超时"));
    }

    /// OCR 成功后应保留截图会话并给出结果提示，而不是立刻关闭窗口
    #[test]
    fn test_ocr_success_keeps_session_open() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state, SnipperJobKind::Ocr);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Ocr,
            MediaJobOutcome::Ocr {
                text: "识别出来的一段文字".to_string(),
                lines: Vec::new(),
            },
        );

        assert!(
            state.session.outcome.is_none(),
            "OCR 成功不得直接把会话置为结束态"
        );
        let clipboard_id = state
            .pending_clipboard_id()
            .expect("OCR 文本复制应处于等待真实写入状态");
        state.handle_clipboard_finished(clipboard_id, Ok(crate::ports::ClipboardResult::Written));
        let message = state.session.status_message.as_deref().unwrap_or_default();
        assert!(
            message.contains("已复制识别文字"),
            "真实写入完成后应提示识别结果已复制，实际: {message}"
        );
    }

    /// 智能打码结果应立即写入马赛克，无需用户再次确认。
    #[test]
    fn test_redact_outcome_applies_mosaic_immediately() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state, SnipperJobKind::Redact);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Redact,
            MediaJobOutcome::Ocr {
                text: "敏感文字".to_string(),
                lines: vec![crate::ports::OcrTextItem {
                    text: "敏感文字".to_string(),
                    bounds: snapin_foundation::Rect::new(10.0, 20.0, 80.0, 24.0),
                }],
            },
        );
        assert!(state.ocr.redaction_preview.is_some());
        assert!(matches!(
            state.editor.annotations.undo_stack(),
            [AnnotationShape::Mosaic(_)]
        ));
    }

    /// 二维码结果应保留在状态中供后续打开链接。
    #[test]
    fn test_qr_outcome_keeps_result_for_link_action() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state, SnipperJobKind::Qr);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Qr,
            MediaJobOutcome::QrCodes {
                codes: vec![crate::ports::QrCodeItem {
                    text: "https://example.com".to_string(),
                    bounds: snapin_foundation::RectI::new(1, 2, 30, 30),
                }],
            },
        );
        assert_eq!(
            state
                .ocr
                .qr_result
                .as_ref()
                .and_then(|codes| codes.first())
                .map(|code| code.text.as_str()),
            Some("https://example.com")
        );
    }

    /// 测试用图像必须与真实任务使用相同的 RGBA 布局
    #[test]
    fn test_media_job_accepts_rgba_image() {
        let image = image::RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        let job = MediaJob::Ocr {
            image,
            origin: (0, 0),
        };
        assert!(matches!(job, MediaJob::Ocr { .. }));
    }
}

#[cfg(test)]
mod stale_tests {
    use super::*;
    use crate::ports::JobId;

    /// 旧任务的迟到回调不得清空当前任务或改写会话状态。
    #[test]
    fn test_stale_job_outcome_is_ignored() {
        let mut state = SnipperState::new();
        state.begin_pending_job(SnipperJobKind::Save, JobId::new(2), Box::new(|| {}));

        handle_job_outcome(
            &mut state,
            JobId::new(1),
            SnipperJobKind::Save,
            MediaJobOutcome::Failed {
                message: "旧任务失败".to_string(),
            },
        );

        assert_eq!(state.pending_job_id(), Some(JobId::new(2)));
        assert!(state.session.outcome.is_none());
        assert!(state.session.status_message.is_none());
    }
}
