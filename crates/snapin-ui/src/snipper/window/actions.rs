//! 截图结果输出动作处理 (复制到剪贴板、保存文件、转为贴图)。

use super::events::SnipperState;
use super::types::SnipperOutcome;
use crate::paster::PasterItem;
use crate::snipper::selection::SelectionState;
use image::{GenericImageView, RgbaImage};
use snapin_platform::{
    generate_unique_id, save_image_with_config, save_last_selection, write_image_to_clipboard,
};
use std::path::PathBuf;

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
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.selection {
            save_last_selection((rect.x, rect.y, rect.width, rect.height));
        }
        if let Err(e) = write_image_to_clipboard(&cropped) {
            tracing::warn!("复制选区图像到系统剪贴板失败: {e}");
        }
        state.outcome = Some(SnipperOutcome::Copied(cropped));
    }
}

/// 保存选区图像至文件
pub fn handle_action_save(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.selection {
            save_last_selection((rect.x, rect.y, rect.width, rect.height));
        }

        let out_path = match save_image_with_config(&cropped, &state.config.output) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("按配置保存截图失败: {e}，尝试降级保存至用户 Pictures 目录");
                let pictures_dir = std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join("Pictures"))
                    .unwrap_or_else(|_| PathBuf::from("."));
                let p = pictures_dir.join(format!("snapin_{}.png", generate_unique_id()));
                if let Err(save_err) = cropped.save(&p) {
                    tracing::error!("降级保存截图失败: {save_err}");
                }
                p
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
    let pin_pos = match &state.selection {
        SelectionState::Locked { rect, .. } => (rect.x as i32, rect.y as i32),
        _ => return,
    };

    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let Some(mut pixmap) = tiny_skia::Pixmap::new(cropped.width(), cropped.height()) {
            pixmap.data_mut().copy_from_slice(cropped.as_raw());
            let id = generate_unique_id();
            let item = PasterItem::new(id, format!("pin_{id}"), pixmap, pin_pos);
            state.outcome = Some(SnipperOutcome::Pinned(item));
        }
    }
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
}
