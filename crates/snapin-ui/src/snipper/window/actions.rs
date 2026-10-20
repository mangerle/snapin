//! 截图结果输出动作处理 (复制到剪贴板、保存文件、转为贴图)。

use super::events::SnipperState;
use super::jobs::{SnipperJob, SnipperJobKind};
use super::types::SnipperOutcome;
use crate::paster::PasterItem;
use crate::snipper::selection::SelectionState;
use image::{GenericImageView, RgbaImage};
use snapin_platform::{generate_unique_id, save_last_selection, write_image_to_clipboard};

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
        if let SelectionState::Locked { rect, .. } = &state.selection {
            save_last_selection((rect.x, rect.y, rect.width, rect.height));
        }
        match write_image_to_clipboard(&cropped) {
            Ok(()) => state.outcome = Some(SnipperOutcome::CopiedImage),
            Err(e) => {
                tracing::warn!("复制选区图像到系统剪贴板失败: {e}");
                state.set_status_message(format!("复制图像失败: {e}"));
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
        let origin = match &state.selection {
            SelectionState::Locked { rect, .. } => {
                let (x, y, _, _) =
                    rect.to_pixel_bounds(original_rgba.width(), original_rgba.height());
                (x, y)
            }
            _ => return,
        };

        if let SelectionState::Locked { rect, .. } = &state.selection {
            save_last_selection((rect.x, rect.y, rect.width, rect.height));
        }

        dispatch_job(
            state,
            SnipperJob {
                kind: SnipperJobKind::Save,
                image: cropped,
                origin,
                cover_mode: state.config.translate.cover_mode,
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
        match tiny_skia::Pixmap::new(cropped.width(), cropped.height()) {
            Some(mut pixmap) => {
                pixmap.data_mut().copy_from_slice(cropped.as_raw());
                let id = generate_unique_id();
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

/// 执行选区 OCR 文字识别并静默复制全文至系统剪贴板
pub fn handle_action_ocr(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    if state.pending_job.is_some() {
        state.set_status_message("请等待当前任务完成");
        return;
    }

    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        let origin = match &state.selection {
            SelectionState::Locked { rect, .. } => {
                let (x, y, _, _) =
                    rect.to_pixel_bounds(original_rgba.width(), original_rgba.height());
                (x, y)
            }
            _ => return,
        };

        if let SelectionState::Locked { rect, .. } = &state.selection {
            save_last_selection((rect.x, rect.y, rect.width, rect.height));
        }

        dispatch_job(
            state,
            SnipperJob {
                kind: SnipperJobKind::Ocr,
                image: cropped,
                origin,
                cover_mode: state.config.translate.cover_mode,
            },
        );
    }
}

/// 将任务提交到后台并更新等待状态
fn dispatch_job(state: &mut SnipperState, job: SnipperJob) {
    let kind = job.kind;
    let Some(dispatcher) = state.job_dispatcher.clone() else {
        state.set_status_message("后台任务调度器不可用");
        return;
    };

    match dispatcher.dispatch(job) {
        Some(abort_handle) => {
            state.pending_job = Some(kind);
            state.pending_job_abort = Some(abort_handle);
            state.set_status_message(format!("正在执行{}...", kind.label()));
        }
        None => state.set_status_message("后台任务调度器不可用"),
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
        SnipperJob {
            kind: SnipperJobKind::Translate,
            image: cropped,
            origin: (px, py),
            cover_mode: state.config.translate.cover_mode,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snipper::selection::Rect;
    use image::Rgba;
    use snapin_core::annotation::action::AnnotationShape;
    use snapin_core::annotation::shapes::RectAction;

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
            color: (255, 0, 0, 255),
            stroke_width: 1.0,
            is_filled: true,
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
}
