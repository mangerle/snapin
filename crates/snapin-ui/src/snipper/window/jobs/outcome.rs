//! 截图任务结果回填与状态流转。

use super::SnipperJobKind;
use crate::ports::{JobId, MediaJobOutcome};
use crate::snipper::window::ocr_panel::OcrPanelState;
use crate::snipper::window::redaction::RedactionPreview;
use crate::snipper::window::state::SnipperState;
use crate::snipper::window::types::SnipperOutcome;
use snapin_core::annotation::action::AnnotationShape;
use snapin_core::annotation::shapes::TranslationAction;

/// 处理后台任务结果，驱动截图会话状态流转。
pub fn handle_job_outcome(
    state: &mut SnipperState,
    job_id: JobId,
    kind: SnipperJobKind,
    outcome: MediaJobOutcome,
) {
    if state.pending_job_id != Some(job_id) {
        tracing::debug!("丢弃过期的截图媒体任务结果: {job_id:?}");
        return;
    }
    state.pending_job = None;
    state.pending_job_id = None;
    state.pending_job_cancel = None;

    match outcome {
        MediaJobOutcome::Ocr { text, lines } => {
            if kind == SnipperJobKind::Redact {
                if lines.is_empty() {
                    state.set_status_message(snapin_core::i18n::tr(
                        "未识别到文字，无法生成打码区域",
                    ));
                    return;
                }
                let count = lines.len();
                state.redaction_preview = Some(RedactionPreview::from_ocr_lines(&lines));
                state.set_status_message(format!(
                    "识别到 {count} 行文字；Enter 应用马赛克，M 马赛克，B 模糊，Esc 取消"
                ));
                return;
            }
            state.ocr_panel = Some(OcrPanelState::from_ocr_lines(&lines));
            let text = text.trim();
            if text.is_empty() {
                state.set_status_message(snapin_core::i18n::tr("未识别到文字"));
                return;
            }
            match state.desktop.write_text_clipboard(text) {
                Ok(()) => {
                    // 识别结果只写入剪贴板，不结束截图会话：用户常常要在同一张图上继续
                    // 标注或再识别另一块区域，直接关闭会让人来不及确认结果。
                    state.set_status_message(snapin_core::trf!(
                        "已复制识别文字（{} 字符）",
                        "Recognized text copied ({} characters)",
                        text.chars().count()
                    ));
                }
                Err(error) => {
                    tracing::warn!("复制 OCR 文本至系统剪贴板失败: {error}");
                    state.set_status_message(snapin_core::trf!(
                        "复制识别文字失败: {}",
                        "Failed to copy recognized text: {}",
                        error
                    ));
                }
            }
        }
        MediaJobOutcome::QrCodes { codes } => {
            if codes.is_empty() {
                state.qr_result = None;
                state.set_status_message(snapin_core::i18n::tr("未识别到二维码"));
                return;
            }
            let count = codes.len();
            let first = codes[0].text.clone();
            state.qr_result = Some(codes);
            match state.desktop.write_text_clipboard(&first) {
                Ok(()) => {
                    let suffix = if first.starts_with("http://") || first.starts_with("https://") {
                        snapin_core::i18n::tr("；Ctrl+Enter 打开链接")
                    } else {
                        ""
                    };
                    state.set_status_message(snapin_core::trf!(
                        "已复制二维码内容（共 {} 个）{}",
                        "QR content copied ({} codes){}",
                        count,
                        suffix
                    ));
                }
                Err(error) => {
                    tracing::warn!("复制二维码文本至系统剪贴板失败: {error}");
                    state.set_status_message(snapin_core::trf!(
                        "复制二维码内容失败: {}",
                        "Failed to copy QR content: {}",
                        error
                    ));
                }
            }
        }
        MediaJobOutcome::Translated { items } => {
            if items.is_empty() {
                state.set_status_message(snapin_core::i18n::tr("未获得有效翻译结果"));
                return;
            }
            state
                .annotations
                .push(AnnotationShape::Translation(TranslationAction::new(items)));
            state.toolbar.active_tool = None;
            state.toolbar.sub_rect = None;
            state.clear_status_message();
            tracing::info!("原位翻译完成，已成功压入标注动作栈");
        }
        MediaJobOutcome::Saved { path } => {
            if kind == SnipperJobKind::Export {
                state.set_status_message(snapin_core::trf!(
                    "OCR 结果已导出至: {}",
                    "OCR results exported to: {}",
                    path.display()
                ));
            } else {
                state.outcome = Some(SnipperOutcome::Saved(path));
            }
        }
        MediaJobOutcome::SaveCancelled => {
            state.set_status_message(snapin_core::i18n::tr("已取消保存"));
        }
        MediaJobOutcome::Failed { message } => {
            tracing::warn!("{}后台任务失败: {message}", kind.label());
            let message = snapin_core::i18n::localize_error(&message);
            state.set_status_message(snapin_core::trf!(
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
    use crate::ports::MediaJob;
    use image::Rgba;
    use snapin_core::annotation::shapes::TranslationShapeItem;
    use snapin_core::config::CoverMode;
    use std::path::PathBuf;

    fn translation_item() -> TranslationShapeItem {
        TranslationShapeItem {
            rect: (10.0, 10.0, 40.0, 20.0),
            original: "Hello".to_string(),
            translated: "你好".to_string(),
            bg_color: (255, 255, 255, 255).into(),
            text_color: (0, 0, 0, 255).into(),
            font_size: 14.0,
            cover_mode: CoverMode::SolidColor,
        }
    }

    /// 让状态接受一个测试任务 ID，模拟真实提交成功后的待处理态。
    fn accept_job(state: &mut SnipperState) -> JobId {
        let job_id = JobId::new(1);
        state.pending_job_id = Some(job_id);
        job_id
    }

    #[test]
    fn test_handle_empty_ocr_outcome_keeps_session_open() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Ocr,
            MediaJobOutcome::Ocr {
                text: String::new(),
                lines: Vec::new(),
            },
        );
        assert!(state.outcome.is_none());
        assert_eq!(state.status_message.as_deref(), Some("未识别到文字"));
    }

    #[test]
    fn test_handle_translate_outcome_pushes_annotation() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Translate,
            MediaJobOutcome::Translated {
                items: vec![translation_item()],
            },
        );
        assert!(state.annotations.can_undo());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_handle_saved_outcome_sets_result() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        let path = PathBuf::from("/tmp/snapin_test.png");
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Save,
            MediaJobOutcome::Saved { path: path.clone() },
        );
        match state.outcome {
            Some(SnipperOutcome::Saved(saved_path)) => assert_eq!(saved_path, path),
            _ => panic!("保存成功必须返回 Saved 结果"),
        }
    }

    #[test]
    fn test_handle_save_cancelled_keeps_session_open() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Save,
            MediaJobOutcome::SaveCancelled,
        );
        assert!(state.outcome.is_none(), "取消保存不得结束截图会话");
        assert_eq!(state.status_message.as_deref(), Some("已取消保存"));
    }

    #[test]
    fn test_handle_job_failure_sets_status() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Translate,
            MediaJobOutcome::Failed {
                message: "网络超时".to_string(),
            },
        );
        assert!(state
            .status_message
            .as_deref()
            .unwrap_or_default()
            .contains("网络超时"));
    }

    /// OCR 成功后应保留截图会话并给出结果提示，而不是立刻关闭窗口
    #[test]
    fn test_ocr_success_keeps_session_open() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Ocr,
            MediaJobOutcome::Ocr {
                text: "识别出来的一段文字".to_string(),
                lines: Vec::new(),
            },
        );

        assert!(state.outcome.is_none(), "OCR 成功不得直接把会话置为结束态");
        let message = state.status_message.as_deref().unwrap_or_default();
        assert!(
            message.contains("已复制识别文字"),
            "应提示识别结果已复制，实际: {message}"
        );
    }

    /// 智能打码结果只生成预览，用户确认前不得改写标注历史。
    #[test]
    fn test_redact_outcome_waits_for_confirmation() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Redact,
            MediaJobOutcome::Ocr {
                text: "敏感文字".to_string(),
                lines: vec![crate::ports::OcrTextItem {
                    text: "敏感文字".to_string(),
                    bounds: (10.0, 20.0, 80.0, 24.0),
                }],
            },
        );
        assert!(state.redaction_preview.is_some());
        assert!(!state.annotations.can_undo());
    }

    /// 二维码结果应保留在状态中供后续打开链接。
    #[test]
    fn test_qr_outcome_keeps_result_for_link_action() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Qr,
            MediaJobOutcome::QrCodes {
                codes: vec![crate::ports::QrCodeItem {
                    text: "https://example.com".to_string(),
                    bounds: (1, 2, 30, 30),
                }],
            },
        );
        assert_eq!(
            state
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
        state.pending_job = Some(SnipperJobKind::Save);
        state.pending_job_id = Some(JobId::new(2));

        handle_job_outcome(
            &mut state,
            JobId::new(1),
            SnipperJobKind::Save,
            MediaJobOutcome::Failed {
                message: "旧任务失败".to_string(),
            },
        );

        assert_eq!(state.pending_job_id, Some(JobId::new(2)));
        assert!(state.outcome.is_none());
        assert!(state.status_message.is_none());
    }
}
