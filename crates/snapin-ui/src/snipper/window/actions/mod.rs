//! 截图结果输出动作处理 (复制到剪贴板、保存文件、转为贴图)。

use super::clipboard::SnipperClipboardAction;
use super::events::SnipperState;
use super::jobs::SnipperJobKind;
use crate::ports::MediaJob;
use crate::snipper::selection::{Rect, SelectionState};
use image::{GenericImageView, RgbaImage};
use snapin_render::{pixmap_into_rgba_image, rgba_image_to_pixmap};

mod pin;
pub use pin::handle_action_pin;
pub(crate) use pin::{build_pin_item, evaluate_pin_size_policy, PinSizeDecision};

/// 裁剪当前选区图像并合成已绘制的矢量标注
pub fn crop_selection_image(
    state: &mut SnipperState,
    original_rgba: &RgbaImage,
) -> Option<RgbaImage> {
    // 提交未完成的标注或文字输入
    super::annotation_builder::commit_annotation(state);
    super::events::commit_active_text_input(state);

    if let Some(session) = &state.long_capture {
        let long_canvas = session.stitcher.canvas();
        if !state.editor.annotations.can_undo() {
            return Some(long_canvas.clone());
        }
        let mut pixmap = rgba_image_to_pixmap(long_canvas)?;
        state.editor.annotations.replay_all(&mut pixmap.as_mut());
        return pixmap_into_rgba_image(pixmap);
    }

    if let SelectionState::Locked { rect, .. } = &state.viewport.selection {
        let (px, py, pw, ph) = rect.to_pixel_bounds(original_rgba.width(), original_rgba.height());
        if pw > 0 && ph > 0 {
            // 无论是否有标注，首先仅截取选区尺寸的子图
            let sub_image = original_rgba.view(px, py, pw, ph).to_image();
            if !state.editor.annotations.can_undo() {
                return Some(sub_image);
            }

            // 存在标注时，仅分配选区尺寸大小的 Pixmap，基于负偏移量局部回放图元，节约数十倍瞬时内存与拷贝
            let mut cropped_pixmap = rgba_image_to_pixmap(&sub_image)?;
            state.editor.annotations.replay_with_offset(
                &mut cropped_pixmap.as_mut(),
                -(px as f32),
                -(py as f32),
            );

            return pixmap_into_rgba_image(cropped_pixmap);
        }
    }
    None
}

/// 复制选区图像到剪贴板并准备退出。
///
/// 开启美化时把渲染提交到后台媒体执行器，GUI 线程只负责提交与回填结果；
/// 未开启美化时直接复用已合成的选区图像。
pub fn handle_action_copy(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    if state.session.config.beautify.enabled && state.has_pending_job() {
        state.set_status_message(snapin_foundation::i18n::tr("请等待当前任务完成"));
        return;
    }
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.viewport.selection {
            state.remember_selection(*rect);
        }
        if state.session.config.beautify.enabled {
            dispatch_job(
                state,
                SnipperJobKind::Copy,
                MediaJob::Beautify {
                    image: cropped,
                    options: (&state.session.config.beautify).into(),
                },
            );
        } else {
            state.submit_clipboard_image(cropped, SnipperClipboardAction::CopyImage);
        }
    }
}

/// 保存选区图像至文件
pub fn handle_action_save(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    if state.has_pending_job() {
        state.set_status_message(snapin_foundation::i18n::tr("请等待当前任务完成"));
        return;
    }

    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.viewport.selection {
            state.remember_selection(*rect);
        }
        if state.session.config.beautify.enabled {
            dispatch_job(
                state,
                SnipperJobKind::Beautify,
                MediaJob::SaveBeautified {
                    image: cropped,
                    options: (&state.session.config.beautify).into(),
                },
            );
        } else {
            dispatch_job(
                state,
                SnipperJobKind::Save,
                MediaJob::SaveScreenshot { image: cropped },
            );
        }
    }
}

/// 按当前美化设置直接保存截图，供全局美化热键使用。
pub fn handle_action_save_beautified(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    if state.has_pending_job() {
        state.set_status_message(snapin_foundation::i18n::tr("请等待当前任务完成"));
        return;
    }
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.viewport.selection {
            state.remember_selection(*rect);
        }
        dispatch_job(
            state,
            SnipperJobKind::Beautify,
            MediaJob::SaveBeautified {
                image: cropped,
                options: (&state.session.config.beautify).into(),
            },
        );
    }
}

/// 取色器单击：读取光标处像素，按用户设置的格式复制颜色并退出。
pub fn handle_action_pick_color(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if state.long_capture.is_some() {
        return;
    }
    state.clear_status_message();
    let text = color_text_at_cursor(state, original_rgba);
    state.submit_clipboard_text(text.clone(), SnipperClipboardAction::PickColor(text));
}

/// 读取光标位置颜色并按当前取色格式输出；坐标越界时夹取到屏幕边缘。
fn color_text_at_cursor(state: &SnipperState, img: &RgbaImage) -> String {
    let px =
        (state.viewport.cursor_pos.0 as i32).clamp(0, img.width().saturating_sub(1) as i32) as u32;
    let py =
        (state.viewport.cursor_pos.1 as i32).clamp(0, img.height().saturating_sub(1) as i32) as u32;
    let pixel = img.get_pixel(px, py);
    state
        .viewport
        .magnifier
        .format
        .format_color(pixel[0], pixel[1], pixel[2])
}

/// 执行选区 OCR，成功写入剪贴板后关闭截图窗口。
pub fn handle_action_ocr(state: &mut SnipperState, original_rgba: &RgbaImage) {
    dispatch_ocr_job(state, original_rgba, SnipperJobKind::Ocr);
}

/// 执行选区 OCR，并在完成后打开包含全部文字的多行编辑框。
pub fn handle_action_ocr_edit(state: &mut SnipperState, original_rgba: &RgbaImage) {
    dispatch_ocr_job(state, original_rgba, SnipperJobKind::OcrEdit);
}

/// 按目标交互模式提交同一个 OCR 媒体任务。
fn dispatch_ocr_job(state: &mut SnipperState, original_rgba: &RgbaImage, kind: SnipperJobKind) {
    if state.long_capture.is_some() {
        return;
    }
    state.clear_status_message();
    if state.has_pending_job() {
        state.set_status_message(snapin_foundation::i18n::tr("请等待当前任务完成"));
        return;
    }

    let origin = locked_selection_origin(state, original_rgba);
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let Some(origin) = origin {
            state.remember_selection(Rect {
                x: origin.0 as f32,
                y: origin.1 as f32,
                width: cropped.width() as f32,
                height: cropped.height() as f32,
            });
            dispatch_job(
                state,
                kind,
                MediaJob::Ocr {
                    image: cropped,
                    origin,
                },
            );
        }
    }
}

/// 执行智能打码 OCR，完成后先进入遮挡样式预览。
pub fn handle_action_redact(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if state.long_capture.is_some() {
        return;
    }
    state.clear_status_message();
    if state.has_pending_job() {
        state.set_status_message(snapin_foundation::i18n::tr("请等待当前任务完成"));
        return;
    }
    let Some(origin) = locked_selection_origin(state, original_rgba) else {
        return;
    };
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        dispatch_job(
            state,
            SnipperJobKind::Redact,
            MediaJob::Ocr {
                image: cropped,
                origin,
            },
        );
    }
}

/// 识别选区中的全部二维码，并将首个结果复制到剪贴板。
pub fn handle_action_qr(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if state.long_capture.is_some() {
        return;
    }
    state.clear_status_message();
    if state.has_pending_job() {
        state.set_status_message(snapin_foundation::i18n::tr("请等待当前任务完成"));
        return;
    }
    let Some(origin) = locked_selection_origin(state, original_rgba) else {
        return;
    };
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        dispatch_job(
            state,
            SnipperJobKind::Qr,
            MediaJob::DecodeQr {
                image: cropped,
                origin,
            },
        );
    }
}

/// 返回当前锁定选区在原始截图中的像素原点。
fn locked_selection_origin(state: &SnipperState, original_rgba: &RgbaImage) -> Option<(u32, u32)> {
    let SelectionState::Locked { rect, .. } = &state.viewport.selection else {
        return None;
    };
    let (x, y, width, height) = rect.to_pixel_bounds(original_rgba.width(), original_rgba.height());
    (width > 0 && height > 0).then_some((x, y))
}

/// 将任务提交到应用层媒体执行器并更新等待状态。
///
/// UI 不自行创建线程或 Tokio 任务；取消与结果回传统一由截图任务运行时负责。
pub(super) fn dispatch_job(state: &mut SnipperState, kind: SnipperJobKind, job: MediaJob) {
    dispatch_job_at(state, kind, job, None);
}

/// 提交任务的统一实现；`pin_position` 只允许与 `Pin` 任务同时出现。
fn dispatch_job_at(
    state: &mut SnipperState,
    kind: SnipperJobKind,
    job: MediaJob,
    pin_position: Option<(i32, i32)>,
) {
    let Some(runtime) = state.jobs.runtime.clone() else {
        state.set_status_message(snapin_foundation::i18n::tr("后台任务媒体执行器不可用"));
        return;
    };

    match runtime.submit(kind, job) {
        Ok((job_id, cancel)) => {
            match pin_position {
                Some(position) => state.begin_pin_job(job_id, cancel, position),
                None => state.begin_pending_job(kind, job_id, cancel),
            }
            if kind == SnipperJobKind::Redact {
                state.clear_status_message();
            } else {
                state.set_status_message(format!("正在执行{}...", kind.label()));
            }
        }
        Err(error) => {
            tracing::warn!("提交{}任务失败: {error}", kind.label());
            state.set_status_message(format!("提交{}任务失败: {error}", kind.label()));
        }
    }
}
/// 执行选区截图原位翻译替换
pub fn handle_action_translate(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if state.long_capture.is_some() {
        return;
    }
    state.clear_status_message();
    if state.has_pending_job() {
        state.set_status_message(snapin_foundation::i18n::tr("请等待当前任务完成"));
        return;
    }

    let (px, py, pw, ph) = match &state.viewport.selection {
        SelectionState::Locked { rect, .. } => {
            rect.to_pixel_bounds(original_rgba.width(), original_rgba.height())
        }
        _ => return,
    };
    if pw == 0 || ph == 0 {
        return;
    }

    // 获取当前选区裁切图像 (原图 + 已绘制标注)
    let cropped = match crop_selection_image(state, original_rgba) {
        Some(cropped) => cropped,
        None => return,
    };

    dispatch_job(
        state,
        SnipperJobKind::Translate,
        MediaJob::Translate {
            image: cropped,
            origin: (px, py),
            cover_mode: state.session.config.translate.cover_mode,
        },
    );
}

/// 开启长截图滚动拼接模式
pub fn handle_action_long_capture(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if state.recording.is_some() {
        state.set_status_message(snapin_foundation::i18n::tr(
            "请先结束屏幕录制，再进入长截图",
        ));
        return;
    }
    if state.long_capture.is_some() {
        state.set_status_message(snapin_foundation::i18n::tr("长截图已在进行中"));
        return;
    }
    state.clear_status_message();
    let (px, py, pw, ph) = match &state.viewport.selection {
        SelectionState::Locked { rect, .. } => {
            rect.to_pixel_bounds(original_rgba.width(), original_rgba.height())
        }
        _ => return,
    };
    if pw < 10 || ph < 10 {
        state.set_status_message(snapin_foundation::i18n::tr("选区过小，无法进入长截图"));
        return;
    }

    // 进入长截图前彻底清空普通截图阶段遗留的标注草稿、工具激活状态与默认工具栏扩展项。
    state.editor.annotation.active_text_input = None;
    state.editor.annotation.is_selecting_text = false;
    state.editor.annotation.stroke.clear();
    state.editor.annotation.toolbar.active_tool = None;
    state.editor.annotation.toolbar.sub_rect = None;
    state.editor.annotation.toolbar.restore_default_items();
    state.editor.selected_annotation = None;
    state.editor.annotation_drag = None;
    state.editor.annotations.clear();

    let first_frame = original_rgba.view(px, py, pw, ph).to_image();
    state.long_capture = Some(super::state::LongCaptureSession::new(first_frame));
    state.set_status_message(snapin_foundation::i18n::tr(
        "长截图录制中：在选区内滚动鼠标自动拼接",
    ));
}

#[cfg(test)]
mod tests;

/// 从当前锁定选区进入屏幕录制工具栏。
///
/// 打开工具栏时不会立即采集，用户可以继续微调选区，再点击红色录制按钮提交后台任务。
/// 录制区域会向下取整为偶数像素，保证 OpenH264 的 YUV420 输入满足宽高整除约束。
pub fn handle_action_start_recording(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if state.recording.is_some() {
        return;
    }
    if state.long_capture.is_some() {
        state.set_status_message(snapin_foundation::i18n::tr(
            "请先完成或关闭长截图，再开始屏幕录制",
        ));
        return;
    }
    state.clear_status_message();
    let (_, _, pw, ph) = match &state.viewport.selection {
        SelectionState::Locked { rect, .. } => {
            rect.to_pixel_bounds(original_rgba.width(), original_rgba.height())
        }
        _ => return,
    };
    let width = pw & !1;
    let height = ph & !1;
    if width < 2 || height < 2 {
        state.set_status_message(snapin_foundation::i18n::tr("选区过小，无法开始屏幕录制"));
        return;
    }
    state.recording = Some(super::state::RecordingSession::new(
        state.session.recorder.clone(),
        state.session.recording_events.clone(),
    ));
    state.set_status_message(snapin_foundation::i18n::tr(
        "屏幕录制工具栏已打开，点击录制按钮开始",
    ));
}
