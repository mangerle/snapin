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

/// OCR 左键路径必须在剪贴板真实写入成功后设置 CopiedText 并关闭截图会话。
#[test]
fn test_ocr_success_closes_after_clipboard_write() {
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
    assert!(matches!(
        state.session.outcome,
        Some(SnipperOutcome::CopiedText(ref text)) if text == "识别出来的一段文字"
    ));
}

/// 编辑路径不复制文字，而是打开一个包含完整 OCR 文本的多行编辑框。
#[test]
fn test_ocr_edit_outcome_opens_full_text_editor() {
    let mut state = SnipperState::new();
    let job_id = accept_job(&mut state, SnipperJobKind::OcrEdit);
    handle_job_outcome(
        &mut state,
        job_id,
        SnipperJobKind::OcrEdit,
        MediaJobOutcome::Ocr {
            text: "第一行\n第二行".to_string(),
            lines: Vec::new(),
        },
    );

    let editor = state.ocr.editor.as_ref().expect("编辑模式应打开全文编辑框");
    assert_eq!(editor.finish_text(), "第一行\n第二行");
    assert!(state.session.outcome.is_none());
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

/// 美化复制结果必须走真实剪贴板写入，写入成功后才结束截图会话。
#[test]
fn test_beautified_copy_outcome_writes_image_to_clipboard() {
    let mut state = SnipperState::new();
    let job_id = accept_job(&mut state, SnipperJobKind::Copy);
    let image = image::RgbaImage::from_pixel(4, 3, Rgba([1, 2, 3, 255]));
    handle_job_outcome(
        &mut state,
        job_id,
        SnipperJobKind::Copy,
        MediaJobOutcome::Beautified { image },
    );

    assert!(state.session.outcome.is_none(), "写入完成前不得结束会话");
    let clipboard_id = state
        .pending_clipboard_id()
        .expect("美化复制应处于等待真实写入状态");
    state.handle_clipboard_finished(clipboard_id, Ok(crate::ports::ClipboardResult::Written));
    assert!(matches!(
        state.session.outcome,
        Some(SnipperOutcome::CopiedImage)
    ));
}

/// 美化贴图结果必须使用提交时固化的落点创建窗口。
#[test]
fn test_beautified_pin_outcome_uses_submitted_position() {
    let mut state = SnipperState::new();
    let job_id = JobId::new(9);
    state.begin_pin_job(job_id, Box::new(|| {}), (12, 34));
    let image = image::RgbaImage::from_pixel(2, 2, Rgba([9, 8, 7, 255]));
    handle_job_outcome(
        &mut state,
        job_id,
        SnipperJobKind::Pin,
        MediaJobOutcome::Beautified { image },
    );

    match state.session.outcome {
        Some(SnipperOutcome::Pinned(item)) => {
            assert_eq!(item.position, (12, 34));
            assert_eq!((item.pixmap.width(), item.pixmap.height()), (2, 2));
        }
        _ => panic!("美化贴图必须生成 Pinned 结果"),
    }
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
