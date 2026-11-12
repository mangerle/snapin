//! 贴图输出动作与超大图像安全缩放策略。
//!
//! 防止将极端长截图或超高分辨率选区按 1:1 尺寸创建为独立透明窗口，避免触发 Wayland `wl_shm`、
//! X11 或 GPU 纹理尺寸越界导致崩溃，并在合理阈值内按所在屏幕可视区等比缩放展示。

use super::super::events::SnipperState;
use super::super::jobs::SnipperJobKind;
use super::super::types::SnipperOutcome;
use super::{crop_selection_image, dispatch_job_at};
use crate::model::PasterItem;
use crate::paster::MAX_PIN_WINDOW_DIM;
use crate::ports::MediaJob;
use crate::snipper::selection::SelectionState;
use image::RgbaImage;
use snapin_foundation::i18n::tr;
use snapin_foundation::AppPaths;
use snapin_render::rgba_image_to_pixmap;

/// 贴图原图允许的最大单边像素（两倍窗口单边上限，允许缩放到 0.5x 后仍位于安全表面内）。
const MAX_PIN_SOURCE_DIM: u32 = MAX_PIN_WINDOW_DIM * 2;
/// 贴图原图允许的最大总像素量（约 3200 万像素 / 128 MB RGBA），避免超大常驻位图占用过量内存。
const MAX_PIN_SOURCE_PIXELS: u64 = 32_000_000;
/// 自动适配屏幕时的最小初始可读倍率，过小缩略图应改用保存或复制。
const MIN_PIN_FIT_SCALE: f32 = 0.25;
/// 初始贴图窗口占用所在屏幕视口的最大比例。
const PIN_SCREEN_FIT_RATIO: f32 = 0.85;

/// 超大截图转贴图的尺寸安全策略判定结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PinSizeDecision {
    /// 允许创建贴图，并给出适配当前屏幕的初始显示倍率。
    Allow { scale: f32 },
    /// 超出硬件表面或可读性安全阈值，应阻止创建并提示用户改用保存或复制。
    TooLarge { width: u32, height: u32 },
}

/// 评估将指定尺寸图像转换为置顶贴图窗口时的缩放与拒止策略。
pub(crate) fn evaluate_pin_size_policy(
    width: u32,
    height: u32,
    screen_size: (f32, f32),
) -> PinSizeDecision {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > MAX_PIN_SOURCE_DIM
        || height > MAX_PIN_SOURCE_DIM
        || pixels > MAX_PIN_SOURCE_PIXELS
    {
        return PinSizeDecision::TooLarge { width, height };
    }
    let screen_w = if screen_size.0 > 1.0 {
        screen_size.0
    } else {
        1920.0
    };
    let screen_h = if screen_size.1 > 1.0 {
        screen_size.1
    } else {
        1080.0
    };
    let max_w = (screen_w * PIN_SCREEN_FIT_RATIO).clamp(20.0, MAX_PIN_WINDOW_DIM as f32);
    let max_h = (screen_h * PIN_SCREEN_FIT_RATIO).clamp(20.0, MAX_PIN_WINDOW_DIM as f32);
    let wf = width as f32;
    let hf = height as f32;
    if wf <= max_w && hf <= max_h {
        return PinSizeDecision::Allow { scale: 1.0 };
    }
    let scale = (max_w / wf).min(max_h / hf);
    if scale < MIN_PIN_FIT_SCALE || wf * scale < 20.0 || hf * scale < 20.0 {
        return PinSizeDecision::TooLarge { width, height };
    }
    PinSizeDecision::Allow { scale }
}

/// 将选区转为贴图固定在屏幕。
///
/// 贴图落点在提交时固化；开启美化时由后台线程渲染，结果回来后按同一点位创建窗口。
pub fn handle_action_pin(state: &mut SnipperState, original_rgba: &RgbaImage) {
    state.clear_status_message();
    if state.session.config.beautify.enabled && state.has_pending_job() {
        state.set_status_message(tr("请等待当前任务完成"));
        return;
    }
    let pin_pos = match &state.viewport.selection {
        SelectionState::Locked { rect, .. } => (rect.x as i32, rect.y as i32),
        _ => return,
    };

    if let Some(cropped) = crop_selection_image(state, original_rgba) {
        let fit_scale = match evaluate_pin_size_policy(
            cropped.width(),
            cropped.height(),
            state.viewport.screen_size,
        ) {
            PinSizeDecision::Allow { scale } => scale,
            PinSizeDecision::TooLarge { width, height } => {
                state.set_status_message(format!(
                    "截图尺寸过大 ({width}x{height})，超出贴图安全范围，请改用保存或复制"
                ));
                return;
            }
        };
        if state.session.config.beautify.enabled {
            dispatch_job_at(
                state,
                SnipperJobKind::Pin,
                MediaJob::Beautify {
                    image: cropped,
                    options: (&state.session.config.beautify).into(),
                },
                Some(pin_pos),
            );
            return;
        }
        match build_pin_item(cropped, pin_pos, fit_scale) {
            Some(item) => state.session.outcome = Some(SnipperOutcome::Pinned(item)),
            None => {
                tracing::error!("创建贴图像素缓冲区失败");
                state.set_status_message(tr("创建贴图缓冲区失败"));
            }
        }
    }
}

/// 将已合成的 RGBA 图像按指定初始缩放倍率包装为贴图窗口条目；缓冲区分配失败时返回 `None`。
pub(crate) fn build_pin_item(
    image: RgbaImage,
    position: (i32, i32),
    scale: f32,
) -> Option<PasterItem> {
    let pixmap = rgba_image_to_pixmap(&image)?;
    let id = AppPaths::generate_unique_id();
    let mut item = PasterItem::new(id, format!("pin_{id}"), pixmap, position);
    item.transform.set_scale(scale);
    Some(item)
}
