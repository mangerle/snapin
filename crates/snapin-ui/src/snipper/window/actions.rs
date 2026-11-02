//! 截图结果输出动作处理 (复制到剪贴板、保存文件、转为贴图)。

use super::events::SnipperState;
use super::jobs::SnipperJobKind;
use super::types::SnipperOutcome;
use crate::model::PasterItem;
use crate::ports::{MediaJob, OcrExportFormat};
use crate::snipper::selection::{Rect, SelectionState};
use image::{GenericImageView, RgbaImage};

/// 裁剪当前选区图像并合成已绘制的矢量标注
pub fn crop_selection_image(
    state: &mut SnipperState,
    original_rgba: &RgbaImage,
) -> Option<RgbaImage> {
    // 提交未完成的标注或文字输入
    super::annotation_builder::commit_annotation(state);
    super::events::commit_active_text_input(state);

    if let SelectionState::Locked { rect, .. } = &state.selection {
        let (px, py, pw, ph) = rect.to_pixel_bounds(original_rgba.width(), original_rgba.height());
        if pw > 0 && ph > 0 {
            // 无论是否有标注，首先仅截取选区尺寸的子图
            let sub_image = original_rgba.view(px, py, pw, ph).to_image();
            if !state.annotations.can_undo() {
                return Some(sub_image);
            }

            // 存在标注时，仅分配选区尺寸大小的 Pixmap，基于负偏移量局部回放图元，节约数十倍瞬时内存与拷贝
            let mut cropped_pixmap = tiny_skia::Pixmap::new(pw, ph)?;
            cropped_pixmap
                .data_mut()
                .copy_from_slice(sub_image.as_raw());
            state.annotations.replay_with_offset(
                &mut cropped_pixmap.as_mut(),
                -(px as f32),
                -(py as f32),
            );

            return RgbaImage::from_raw(pw, ph, cropped_pixmap.take());
        }
    }
    None
}

/// 复制选区图像到剪贴板并准备退出
pub fn handle_action_copy(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        let cropped = match apply_configured_beautify(state, cropped) {
            Ok(image) => image,
            Err(message) => {
                state.set_status_message(message);
                return;
            }
        };
        if let SelectionState::Locked { rect, .. } = &state.selection {
            state.remember_selection(*rect);
        }
        match state.desktop.write_image_clipboard(&cropped) {
            Ok(()) => state.outcome = Some(SnipperOutcome::CopiedImage),
            Err(error) => {
                tracing::warn!("复制选区图像到系统剪贴板失败: {error}");
                state.set_status_message(format!("复制图像失败: {error}"));
            }
        }
    }
}

/// 保存选区图像至文件
pub fn handle_action_save(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    if state.pending_job.is_some() {
        state.set_status_message("请等待当前任务完成");
        return;
    }

    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.selection {
            state.remember_selection(*rect);
        }
        if state.config.beautify.enabled {
            dispatch_job(
                state,
                SnipperJobKind::Beautify,
                MediaJob::SaveBeautified {
                    image: cropped,
                    options: (&state.config.beautify).into(),
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
    if state.pending_job.is_some() {
        state.set_status_message("请等待当前任务完成");
        return;
    }
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.selection {
            state.remember_selection(*rect);
        }
        dispatch_job(
            state,
            SnipperJobKind::Beautify,
            MediaJob::SaveBeautified {
                image: cropped,
                options: (&state.config.beautify).into(),
            },
        );
    }
}

/// 将选区转为贴图固定在屏幕
pub fn handle_action_pin(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    let pin_pos = match &state.selection {
        SelectionState::Locked { rect, .. } => (rect.x as i32, rect.y as i32),
        _ => return,
    };

    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        let cropped = match apply_configured_beautify(state, cropped) {
            Ok(image) => image,
            Err(message) => {
                state.set_status_message(message);
                return;
            }
        };
        match tiny_skia::Pixmap::new(cropped.width(), cropped.height()) {
            Some(mut pixmap) => {
                pixmap.data_mut().copy_from_slice(cropped.as_raw());
                let id = snapin_core::AppPaths::generate_unique_id();
                let item = PasterItem::new(id, format!("pin_{id}"), pixmap, pin_pos);
                state.outcome = Some(SnipperOutcome::Pinned(item));
            }
            None => {
                tracing::error!(
                    "创建贴图像素缓冲区失败，尺寸: {}x{}",
                    cropped.width(),
                    cropped.height()
                );
                state.set_status_message("创建贴图缓冲区失败");
            }
        }
    }
}

/// 按配置把最终输出图像转换为美化版本；关闭美化时原样返回。
fn apply_configured_beautify(state: &SnipperState, image: RgbaImage) -> Result<RgbaImage, String> {
    if !state.config.beautify.enabled {
        return Ok(image);
    }
    let mut input = tiny_skia::Pixmap::new(image.width(), image.height())
        .ok_or_else(|| "分配美化输出输入缓冲区失败".to_string())?;
    input.data_mut().copy_from_slice(image.as_raw());
    let output =
        snapin_core::render::render_beautified(input.as_ref(), &(&state.config.beautify).into())?;
    RgbaImage::from_raw(output.width(), output.height(), output.take())
        .ok_or_else(|| "转换美化输出图像失败".to_string())
}

/// 取色器单击：读取光标处像素，按用户设置的格式复制颜色并退出。
pub fn handle_action_pick_color(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    let text = color_text_at_cursor(state, original_rgba);

    match state.desktop.write_text_clipboard(&text) {
        Ok(()) => state.outcome = Some(SnipperOutcome::CopiedColor(text)),
        Err(error) => {
            tracing::warn!("取色器复制颜色值到剪贴板失败: {error}");
            state.set_status_message(format!("复制颜色失败: {error}"));
        }
    }
}

/// 读取光标位置颜色并按当前取色格式输出；坐标越界时夹取到屏幕边缘。
fn color_text_at_cursor(state: &SnipperState, img: &RgbaImage) -> String {
    let px = (state.cursor_pos.0 as i32).clamp(0, img.width().saturating_sub(1) as i32) as u32;
    let py = (state.cursor_pos.1 as i32).clamp(0, img.height().saturating_sub(1) as i32) as u32;
    let pixel = img.get_pixel(px, py);
    state
        .magnifier
        .format
        .format_color(pixel[0], pixel[1], pixel[2])
}

/// 执行选区 OCR 文字识别并静默复制全文至系统剪贴板
pub fn handle_action_ocr(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    if state.pending_job.is_some() {
        state.set_status_message("请等待当前任务完成");
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
                SnipperJobKind::Ocr,
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
    state.clear_status_message();
    if state.pending_job.is_some() {
        state.set_status_message("请等待当前任务完成");
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
    state.clear_status_message();
    if state.pending_job.is_some() {
        state.set_status_message("请等待当前任务完成");
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

/// 重新打开最近一次 OCR 结果面板；没有结果时立即执行一次 OCR。
pub fn handle_action_ocr_panel(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if state.ocr_panel.is_some() {
        state.set_status_message("OCR 结果面板已打开；Esc 关闭");
    } else {
        handle_action_ocr(state, original_rgba);
    }
}

/// 导出 OCR 面板当前选中的文本行。
pub fn handle_action_export_ocr(state: &mut SnipperState, format: OcrExportFormat) {
    if state.pending_job.is_some() {
        state.set_status_message("请等待当前任务完成");
        return;
    }
    let Some(panel) = state.ocr_panel.as_ref() else {
        state.set_status_message("没有可导出的 OCR 结果");
        return;
    };
    let lines = panel.selected_lines();
    if lines.is_empty() {
        state.set_status_message("请至少选择一行 OCR 结果");
        return;
    }
    dispatch_job(
        state,
        SnipperJobKind::Export,
        MediaJob::SaveOcrText { lines, format },
    );
}

/// 返回当前锁定选区在原始截图中的像素原点。
fn locked_selection_origin(state: &SnipperState, original_rgba: &RgbaImage) -> Option<(u32, u32)> {
    let SelectionState::Locked { rect, .. } = &state.selection else {
        return None;
    };
    let (x, y, width, height) = rect.to_pixel_bounds(original_rgba.width(), original_rgba.height());
    (width > 0 && height > 0).then_some((x, y))
}

/// 将任务提交到应用层媒体执行器并更新等待状态。
///
/// UI 不自行创建线程或 Tokio 任务；取消与结果回传统一由截图任务运行时负责。
fn dispatch_job(state: &mut SnipperState, kind: SnipperJobKind, job: MediaJob) {
    let Some(runtime) = state.job_runtime.clone() else {
        state.set_status_message("后台任务媒体执行器不可用");
        return;
    };

    match runtime.submit(kind, job) {
        Ok((job_id, cancel)) => {
            state.pending_job = Some(kind);
            state.pending_job_id = Some(job_id);
            state.pending_job_cancel = Some(cancel);
            state.set_status_message(format!("正在执行{}...", kind.label()));
        }
        Err(error) => {
            tracing::warn!("提交{}任务失败: {error}", kind.label());
            state.set_status_message(format!("提交{}任务失败: {error}", kind.label()));
        }
    }
}

/// 执行选区截图原位翻译替换
pub fn handle_action_translate(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    if state.pending_job.is_some() {
        state.set_status_message("请等待当前任务完成");
        return;
    }

    let (px, py, pw, ph) = match &state.selection {
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
            cover_mode: state.config.translate.cover_mode,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snipper::magnifier::ColorFormat;
    use crate::snipper::selection::Rect;
    use image::Rgba;
    use snapin_core::annotation::action::AnnotationShape;
    use snapin_core::annotation::shapes::RectAction;
    use snapin_core::annotation::LineStyle;

    /// 取色器按 HEX/RGB 格式输出，并在坐标越界时夹取到边缘
    #[test]
    fn test_color_text_at_cursor_formats_and_clamps() {
        let mut state = SnipperState::new();
        let img = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));

        state.cursor_pos = (2.0, 2.0);
        state.magnifier.format = ColorFormat::Hex;
        assert_eq!(color_text_at_cursor(&state, &img), "#FF0000");

        state.magnifier.format = ColorFormat::Rgb;
        assert_eq!(color_text_at_cursor(&state, &img), "rgb(255, 0, 0)");

        state.cursor_pos = (99.0, -5.0);
        assert_eq!(color_text_at_cursor(&state, &img), "rgb(255, 0, 0)");
    }

    #[test]
    fn test_crop_without_selection() {
        let mut state = SnipperState::new();
        let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        assert!(crop_selection_image(&mut state, &original).is_none());
    }

    #[test]
    fn test_crop_without_annotations_returns_original() {
        let mut state = SnipperState::new();
        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 10.0,
                y: 10.0,
                width: 50.0,
                height: 50.0,
            },
            active_drag: None,
        };
        let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        let cropped = crop_selection_image(&mut state, &original).expect("应成功裁切选区");
        assert_eq!(cropped.width(), 50);
        assert_eq!(cropped.height(), 50);
        assert_eq!(cropped.get_pixel(0, 0), &Rgba([255, 255, 255, 255]));
    }

    #[test]
    fn test_crop_with_annotations_composited() {
        let mut state = SnipperState::new();
        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 10.0,
                y: 10.0,
                width: 50.0,
                height: 50.0,
            },
            active_drag: None,
        };

        // 添加一个全红色的填充矩形标注到全屏坐标 (15, 15, 20, 20)
        state.annotations.push(AnnotationShape::Rect(RectAction {
            rect: (15.0, 15.0, 20.0, 20.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 1.0,
            is_filled: true,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));

        let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        let cropped = crop_selection_image(&mut state, &original).expect("应成功裁切合成选区");

        assert_eq!(cropped.width(), 50);
        assert_eq!(cropped.height(), 50);
        // (0, 0) 对应原图 (10, 10)，未被矩形覆盖，应仍为白色
        assert_eq!(cropped.get_pixel(0, 0), &Rgba([255, 255, 255, 255]));
        // (10, 10) 对应原图 (20, 20)，位于矩形内部，应合成红色
        assert_eq!(cropped.get_pixel(10, 10), &Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn test_handle_action_pin_sets_pinned_outcome() {
        let mut state = SnipperState::new();
        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 10.0,
                y: 10.0,
                width: 40.0,
                height: 40.0,
            },
            active_drag: None,
        };
        let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        handle_action_pin(&mut state, &original);

        match &state.outcome {
            Some(SnipperOutcome::Pinned(item)) => {
                assert_eq!(item.position, (10, 10));
                assert_eq!(item.pixmap.width(), 40);
                assert_eq!(item.pixmap.height(), 40);
            }
            _ => panic!("预期输出 Pinned 状态"),
        }
    }

    #[test]
    fn test_handle_action_ocr_empty_image_keeps_session_open() {
        let mut state = SnipperState::new();
        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 50.0,
                height: 50.0,
            },
            active_drag: None,
        };
        let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        handle_action_ocr(&mut state, &original);

        assert!(
            state.outcome.is_none(),
            "空图像或引擎不可用时不得以 Copied 状态退出"
        );
        assert!(
            state.status_message.is_some(),
            "识别失败或空结果必须给出用户提示"
        );
    }

    #[test]
    fn test_handle_action_translate_graceful_handling() {
        let mut state = SnipperState::new();
        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 50.0,
                height: 50.0,
            },
            active_drag: None,
        };
        let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        // 无可用模型或未识别到文字时安全返回，不崩溃，也不得压入破坏性图元
        handle_action_translate(&mut state, &original);
        assert!(!state.annotations.can_undo());
        assert!(state.status_message.is_some());
    }

    #[test]
    fn test_handle_action_translate_zero_or_idle_selection() {
        let mut state = SnipperState::new();
        let original = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        // Idle 状态安全返回
        handle_action_translate(&mut state, &original);

        // 零尺寸锁定状态安全返回
        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 10.0,
                y: 10.0,
                width: 0.0,
                height: 0.0,
            },
            active_drag: None,
        };
        handle_action_translate(&mut state, &original);
        assert!(!state.annotations.can_undo());
    }

    /// 美化开启后，最终输出图像必须采用设置中的尺寸和留白。
    #[test]
    fn test_configured_beautify_changes_final_output() {
        let mut state = SnipperState::new();
        state.config.beautify.enabled = true;
        state.config.beautify.social_size = snapin_core::render::SocialSize::Square;
        let image = RgbaImage::from_pixel(100, 50, Rgba([255, 0, 0, 255]));
        let output = apply_configured_beautify(&state, image).expect("美化输出应成功");
        assert_eq!(output.dimensions(), (1080, 1080));
    }
}
