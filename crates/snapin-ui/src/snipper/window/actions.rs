//! 截图结果输出动作处理 (复制到剪贴板、保存文件、转为贴图)。

use super::events::SnipperState;
use super::types::SnipperOutcome;
use crate::paster::PasterItem;
use crate::snipper::selection::SelectionState;
use image::{GenericImageView, RgbaImage};
use snapin_platform::{
    generate_unique_id, save_image_with_config, save_last_selection, write_image_to_clipboard,
};

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
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.selection {
            save_last_selection((rect.x, rect.y, rect.width, rect.height));
        }

        let out_path = match save_image_with_config(&cropped, &state.config.output) {
            Ok(path) => path,
            Err(primary_error) => {
                tracing::warn!(
                    "按配置保存截图失败: {primary_error}，尝试降级保存至用户 Pictures 目录"
                );
                let pictures_dir = snapin_core::AppPaths::pictures_dir();
                if let Err(create_error) = std::fs::create_dir_all(&pictures_dir) {
                    tracing::error!("创建 Pictures 目录失败: {create_error}");
                    state.set_status_message(format!("保存截图失败: {create_error}"));
                    return;
                }
                let fallback_path =
                    pictures_dir.join(format!("snapin_{}.png", generate_unique_id()));
                if let Err(save_error) = cropped.save(&fallback_path) {
                    tracing::error!("降级保存截图失败: {save_error}");
                    state.set_status_message(format!("保存截图失败: {save_error}"));
                    return;
                }
                fallback_path
            }
        };

        if let Err(e) = write_image_to_clipboard(&cropped) {
            tracing::warn!("保存后同步复制到系统剪贴板失败: {e}");
        }
        state.outcome = Some(SnipperOutcome::Saved(out_path));
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
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.selection {
            save_last_selection((rect.x, rect.y, rect.width, rect.height));
        }

        let custom_dir = if state.config.ocr.local.model_dir.is_empty() {
            None
        } else {
            Some(std::path::Path::new(&state.config.ocr.local.model_dir))
        };

        let engine = match snapin_service::LocalPpOcrEngine::new(custom_dir) {
            Ok(engine) => engine,
            Err(e) => {
                tracing::warn!("本地 OCR 引擎未能初始化: {e}，请检查离线模型文件是否已部署");
                state.set_status_message(format!("OCR 引擎不可用: {e}"));
                return;
            }
        };

        use snapin_service::OcrEngine;
        let result = match engine.recognize(&cropped) {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!("本地 OCR 文字识别失败: {e}");
                state.set_status_message(format!("文字识别失败: {e}"));
                return;
            }
        };

        let text = result.full_text.trim().to_string();
        if text.is_empty() {
            state.set_status_message("未识别到文字");
            return;
        }

        if let Err(e) = snapin_platform::write_text_to_clipboard(&text) {
            tracing::warn!("复制 OCR 文本至系统剪贴板失败: {e}");
            state.set_status_message(format!("复制识别文字失败: {e}"));
            return;
        }

        state.outcome = Some(SnipperOutcome::CopiedText(text));
    }
}

/// 估算指定局部矩形周边的背景主导色 (对四条外边缘进行采样，并采用截尾均值算法剔除边缘深色与高光噪点)
fn estimate_dominant_background_color(
    image: &RgbaImage,
    min_x: u32,
    min_y: u32,
    w: u32,
    h: u32,
) -> (u8, u8, u8, u8) {
    let img_w = image.width();
    let img_h = image.height();
    if img_w == 0 || img_h == 0 {
        return (255, 255, 255, 255);
    }

    // 稍微向外外扩 1 像素采样，避免贴合文字边缘笔画；越界坐标统一夹紧到图内。
    let max_x = img_w.saturating_sub(1);
    let max_y = img_h.saturating_sub(1);
    let x1 = min_x.min(max_x).saturating_sub(1);
    let y1 = min_y.min(max_y).saturating_sub(1);
    let x2 = min_x.saturating_add(w).saturating_add(1).min(max_x);
    let y2 = min_y.saturating_add(h).saturating_add(1).min(max_y);
    if x1 > x2 || y1 > y2 {
        return (255, 255, 255, 255);
    }

    let capacity = ((x2 - x1) as usize)
        .saturating_add((y2 - y1) as usize)
        .saturating_add(2)
        .saturating_mul(2);
    let mut samples = Vec::with_capacity(capacity);

    // 顶部与底部边缘采样
    for x in x1..=x2 {
        let p_top = image.get_pixel(x, y1);
        samples.push((p_top[0], p_top[1], p_top[2]));
        let p_bot = image.get_pixel(x, y2);
        samples.push((p_bot[0], p_bot[1], p_bot[2]));
    }

    // 左侧与右侧边缘采样
    for y in (y1 + 1)..y2 {
        let p_left = image.get_pixel(x1, y);
        samples.push((p_left[0], p_left[1], p_left[2]));
        let p_right = image.get_pixel(x2, y);
        samples.push((p_right[0], p_right[1], p_right[2]));
    }

    if samples.is_empty() {
        return (255, 255, 255, 255);
    }

    // 按灰度亮度排序
    samples.sort_by(|a, b| {
        let lum_a = 0.299 * (a.0 as f32) + 0.587 * (a.1 as f32) + 0.114 * (a.2 as f32);
        let lum_b = 0.299 * (b.0 as f32) + 0.587 * (b.1 as f32) + 0.114 * (b.2 as f32);
        lum_a
            .partial_cmp(&lum_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 截尾均值：剔除前后 15% 离群值（如深色文字笔画或异常反光点）
    let trim_count = (samples.len() as f32 * 0.15) as usize;
    let valid_slice = if samples.len() > trim_count * 2 && trim_count > 0 {
        &samples[trim_count..(samples.len() - trim_count)]
    } else {
        &samples[..]
    };

    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    for &(r, g, b) in valid_slice {
        r_sum += r as u64;
        g_sum += g as u64;
        b_sum += b as u64;
    }

    let count = valid_slice.len() as u64;
    (
        (r_sum / count) as u8,
        (g_sum / count) as u8,
        (b_sum / count) as u8,
        255,
    )
}

/// 计算背景色对应的高反差可读前景色 (浅背景配黑灰，深背景配亮白)
fn get_contrasting_text_color(bg: (u8, u8, u8, u8)) -> (u8, u8, u8, u8) {
    let luminance = 0.299 * (bg.0 as f32) + 0.587 * (bg.1 as f32) + 0.114 * (bg.2 as f32);
    if luminance > 135.0 {
        (30, 30, 30, 255)
    } else {
        (245, 245, 245, 255)
    }
}

/// 执行选区截图原位翻译替换
pub fn handle_action_translate(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
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

    let custom_dir = if state.config.ocr.local.model_dir.is_empty() {
        None
    } else {
        Some(std::path::Path::new(&state.config.ocr.local.model_dir))
    };

    let engine = match snapin_service::LocalPpOcrEngine::new(custom_dir) {
        Ok(engine) => engine,
        Err(e) => {
            tracing::warn!("原位翻译 OCR 引擎未能初始化: {e}");
            state.set_status_message(format!("OCR 引擎不可用: {e}"));
            return;
        }
    };

    use snapin_service::OcrEngine;
    let ocr_res = match engine.recognize(&cropped) {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("原位翻译文字识别失败: {e}");
            state.set_status_message(format!("文字识别失败: {e}"));
            return;
        }
    };

    if ocr_res.blocks.is_empty() {
        tracing::debug!("选区内未检测到有效文字，跳过原位翻译");
        state.set_status_message("选区内未检测到文字");
        return;
    }

    let texts: Vec<String> = ocr_res.blocks.iter().map(|b| b.text.clone()).collect();
    let translator = snapin_service::create_translator(&state.config.translate);
    let target_lang = state.config.translate.target_language.as_str();

    let trans_fut = translator.translate_batch(&texts, "auto", target_lang);
    let trans_result = match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(trans_fut)),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match rt {
                Ok(runtime) => runtime.block_on(trans_fut),
                Err(_) => {
                    state.set_status_message("翻译运行时不可用");
                    return;
                }
            }
        }
    };

    let translated_texts = match trans_result {
        Ok(texts) => texts,
        Err(e) => {
            tracing::warn!("网络翻译服务调用失败: {e}");
            state.set_status_message(format!("翻译失败: {e}"));
            return;
        }
    };

    use snapin_core::annotation::action::AnnotationShape;
    use snapin_core::annotation::shapes::{TranslationAction, TranslationShapeItem};

    let mut shape_items = Vec::with_capacity(ocr_res.blocks.len());

    for (idx, block) in ocr_res.blocks.into_iter().enumerate() {
        let Some(translated) = translated_texts.get(idx) else {
            continue;
        };
        let translated = translated.trim();
        // 空译文或与原文相同的译文不生成图元，避免覆盖原图内容。
        if translated.is_empty() || translated == block.text.trim() {
            continue;
        }

        let (bx, by, bw, bh) = block.bbox;
        let local_w = bw.max(2.0);
        let local_h = bh.max(2.0);

        let bg_color = estimate_dominant_background_color(
            &cropped,
            bx.floor().max(0.0) as u32,
            by.floor().max(0.0) as u32,
            local_w.ceil() as u32,
            local_h.ceil() as u32,
        );
        let text_color = get_contrasting_text_color(bg_color);
        let font_size = (local_h * 0.75).clamp(10.0, 36.0);

        // 转换为全屏绝对坐标
        let abs_x = (px as f32) + bx;
        let abs_y = (py as f32) + by;

        shape_items.push(TranslationShapeItem {
            rect: (abs_x, abs_y, local_w, local_h),
            original: block.text,
            translated: translated.to_string(),
            bg_color,
            text_color,
            font_size,
            cover_mode: state.config.translate.cover_mode,
        });
    }

    if shape_items.is_empty() {
        state.set_status_message("未获得有效翻译结果");
        return;
    }

    state
        .annotations
        .push(AnnotationShape::Translation(TranslationAction::new(
            shape_items,
        )));
    state.toolbar.active_tool = None;
    state.toolbar.sub_rect = None;
    tracing::info!("原位翻译完成，已成功压入标注动作栈");
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
    fn test_estimate_dominant_background_and_contrast() {
        let mut img = RgbaImage::new(100, 100);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([200, 200, 200, 255]);
        }
        let bg = estimate_dominant_background_color(&img, 10, 10, 30, 20);
        assert_eq!(bg, (200, 200, 200, 255));
        let fg = get_contrasting_text_color(bg);
        // 浅背景应该匹配深灰黑文字
        assert_eq!(fg, (30, 30, 30, 255));

        // 深黑背景测试
        let dark_bg = (20, 20, 20, 255);
        let dark_fg = get_contrasting_text_color(dark_bg);
        assert_eq!(dark_fg, (245, 245, 245, 255));
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
