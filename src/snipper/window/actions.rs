//! 截图结果输出动作处理 (复制到剪贴板、保存文件、转为贴图)。

use super::events::SnipperState;
use super::types::SnipperOutcome;
use crate::paster::PasterItem;
use crate::platform::write_image_to_clipboard;
use crate::snipper::selection::SelectionState;
use image::{GenericImageView, RgbaImage};
use std::path::PathBuf;

/// 裁剪当前选区图像
pub fn crop_selection_image(state: &SnipperState, original_rgba: &RgbaImage) -> Option<RgbaImage> {
    if let SelectionState::Locked { rect, .. } = &state.selection {
        let (px, py, pw, ph) = rect.to_pixel_bounds(original_rgba.width(), original_rgba.height());
        if pw > 0 && ph > 0 {
            let sub = original_rgba.view(px, py, pw, ph).to_image();
            return Some(sub);
        }
    }
    None
}

/// 复制选区图像到剪贴板并准备退出
pub fn handle_action_copy(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.selection {
            crate::platform::save_last_selection((rect.x, rect.y, rect.width, rect.height));
        }
        let _ = write_image_to_clipboard(&cropped);
        state.outcome = Some(SnipperOutcome::Copied(cropped));
    }
}

/// 保存选区图像至文件
pub fn handle_action_save(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        if let SelectionState::Locked { rect, .. } = &state.selection {
            crate::platform::save_last_selection((rect.x, rect.y, rect.width, rect.height));
        }

        let out_path = match crate::platform::save_image_with_config(&cropped, &state.config.output)
        {
            Ok(p) => p,
            Err(_) => {
                let pictures_dir = std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join("Pictures"))
                    .unwrap_or_else(|_| PathBuf::from("."));
                let p = pictures_dir.join(format!("snapin_{}.png", chrono_stamp()));
                let _ = cropped.save(&p);
                p
            }
        };

        let _ = write_image_to_clipboard(&cropped);
        state.outcome = Some(SnipperOutcome::Saved(out_path));
    }
}

/// 将选区转为贴图固定在屏幕
pub fn handle_action_pin(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if let SelectionState::Locked { rect, .. } = &state.selection {
        if let Some(cropped) = crop_selection_image(state, original_rgba) {
            if let Some(mut pixmap) = tiny_skia::Pixmap::new(cropped.width(), cropped.height()) {
                pixmap.data_mut().copy_from_slice(cropped.as_raw());
                let id = chrono_stamp();
                let item = PasterItem::new(
                    id,
                    format!("pin_{id}"),
                    pixmap,
                    (rect.x as i32, rect.y as i32),
                );
                state.outcome = Some(SnipperOutcome::Pinned(item));
            }
        }
    }
}

/// 秒级时间戳生成
fn chrono_stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
