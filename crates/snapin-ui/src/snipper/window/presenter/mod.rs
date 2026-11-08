//! 截图全屏窗口呈现器与底层双缓冲光栅化表面管理。

use super::presenter_support::{covering_pixel_bounds, scale_pixmap_to, selected_annotation_view};
use crate::error::UiError;
use crate::paster::surface::PasterSurface;
use crate::snipper::canvas::SnipperCanvas;
use crate::snipper::magnifier::MagnifierViewport;
use crate::snipper::selection::{Rect, SelectionState};
use crate::snipper::window::events::SnipperState;
use image::RgbaImage;
use std::num::NonZeroU32;
use std::sync::Arc;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 放大镜配置签名：任一字段变化都会改变卡片外观或尺寸，必须回退到全量重绘。
type MagnifierSignature = (bool, u32, bool, f32);

/// 上一帧光标覆盖层快照，用于判断能否只重绘局部脏区并精确恢复旧像素。
#[derive(Clone, Copy)]
struct OverlaySnapshot {
    /// 上一帧悬停的窗口；悬停切换会改变整块高亮区域，必须全量重绘
    hovered_window: Option<Rect>,
    /// 上一帧放大镜卡片占据的矩形（含描边余量）
    magnifier_rect: Option<Rect>,
    /// 上一帧状态提示框占据的矩形
    status_rect: Option<Rect>,
    magnifier_signature: MagnifierSignature,
}

/// 结果页当前预览帧的 Tiny-Skia 缓存。
struct PreviewFrameCache {
    artifact_ptr: usize,
    frame_index: usize,
    pixmap: Pixmap,
}

mod support;

#[cfg(test)]
pub use support::create_dimmed_pixmap_fast;
use support::{create_base_pixmap, magnifier_signature};

/// 截图全屏窗口呈现器：管理帧缓冲区与 ARGB8888 共享内存表面呈现。
pub struct SnipperPresenter {
    surface: PasterSurface,
    base_bg: Pixmap,
    render_pixmap: Pixmap,
    /// 上一帧光标覆盖层快照；为空表示下一帧必须全量重绘
    overlay: Option<OverlaySnapshot>,
    /// 当前结果预览帧缓存，避免每次重绘都复制缩略图像素。
    preview_cache: Option<PreviewFrameCache>,
}

/// 截图呈现器初始化参数。
pub struct SnipperPresenterInit<'a> {
    pub window: Arc<Window>,
    pub width: u32,
    pub height: u32,
    pub bg_image: &'a RgbaImage,
    pub dimmed: bool,
}

impl SnipperPresenter {
    /// 初始化截图呈现器与底层共享帧缓冲表面。
    ///
    /// `dimmed` 为 true 时预计算暗化蒙层用于常规截图；取色器传入 false，
    /// 直接使用原始屏幕图作为底图，避免颜色被遮罩改变视觉观感。
    ///
    /// # Errors
    ///
    /// 像素缓冲分配、表面调整或呈现失败时返回 [`UiError`]。
    pub fn try_new(init: SnipperPresenterInit<'_>) -> Result<Self, UiError> {
        let SnipperPresenterInit {
            window,
            width,
            height,
            bg_image,
            dimmed,
        } = init;
        let mut surface = PasterSurface::new(window)?;
        surface.resize(width, height)?;
        let base_bg = create_base_pixmap(bg_image, width, height, dimmed)?;
        // 映射窗口前先预刷背景底图，避免出现瞬间白屏或未初始化内容；
        // 预刷只是过渡画面，失败时由随后的首次正式渲染兜底，故仅记录调试日志。
        if let Err(error) = surface.present(&base_bg) {
            tracing::debug!("预刷截图背景底图失败，等待首次正式渲染覆盖: {error}");
        }
        let render_pixmap =
            Pixmap::new(width, height).ok_or(UiError::PixmapAlloc { width, height })?;
        Ok(Self {
            surface,
            base_bg,
            render_pixmap,
            overlay: None,
            preview_cache: None,
        })
    }

    /// 窗口尺寸变化时重建表面、底图与提交缓冲。
    ///
    /// winit 全屏窗口在显示器分辨率或缩放变化时会收到 `Resized`。若继续使用
    /// 创建时的固定缓冲，`copy_from_slice` 会因长度不一致而持续报错。这里按新尺寸
    /// 缩放底图与背景图像，并清空增量快照，下一帧强制全量重绘。
    ///
    /// # Errors
    ///
    /// 像素缓冲分配、表面调整或呈现失败时返回 [`UiError`]。
    pub fn resize(
        &mut self,
        width: u32,
        height: u32,
        bg_image: &mut RgbaImage,
    ) -> Result<(), UiError> {
        let w = NonZeroU32::new(width).ok_or_else(|| UiError::Softbuffer {
            message: "窗口宽度变为零".to_string(),
        })?;
        let h = NonZeroU32::new(height).ok_or_else(|| UiError::Softbuffer {
            message: "窗口高度变为零".to_string(),
        })?;
        if self.render_pixmap.width() == width && self.render_pixmap.height() == height {
            return Ok(());
        }

        self.surface.resize(w.get(), h.get())?;

        self.base_bg = scale_pixmap_to(&self.base_bg, width, height)
            .ok_or(UiError::PixmapAlloc { width, height })?;
        if (width as i32 - bg_image.width() as i32).abs() <= 64
            && (height as i32 - bg_image.height() as i32).abs() <= 64
        {
            let mut adjusted = RgbaImage::new(width, height);
            let copy_w = bg_image.width().min(width);
            let copy_h = bg_image.height().min(height);
            for y in 0..copy_h {
                for x in 0..copy_w {
                    adjusted.put_pixel(x, y, *bg_image.get_pixel(x, y));
                }
            }
            *bg_image = adjusted;
        } else {
            *bg_image = image::imageops::resize(
                bg_image,
                width,
                height,
                image::imageops::FilterType::Triangle,
            );
        }
        self.render_pixmap =
            Pixmap::new(width, height).ok_or(UiError::PixmapAlloc { width, height })?;
        self.overlay = None;
        Ok(())
    }

    /// 用新的完整屏幕帧替换预计算底图，并强制下一帧全量重绘。
    ///
    /// 长截图每次滚动抓屏后都需要同步显示当前底层画面，否则选区会停留在旧快照，
    /// 用户无法判断目标应用是否已滚动到期望位置。只替换底图而不重建表面，
    /// 避免高频抓屏时反复分配窗口帧缓冲。
    ///
    /// # Errors
    ///
    /// 新底图尺寸与灰度蒙层分配失败时返回 [`UiError::PixmapAlloc`]。
    pub fn replace_background(
        &mut self,
        bg_image: &RgbaImage,
        dimmed: bool,
    ) -> Result<(), UiError> {
        let width = self.render_pixmap.width();
        let height = self.render_pixmap.height();
        self.base_bg = create_base_pixmap(bg_image, width, height, dimmed)?;
        self.overlay = None;
        Ok(())
    }

    /// 执行全屏离屏光栅化并将成果绘制呈现至物理屏幕表面。
    ///
    /// 空闲态（未选区）下光标移动只改动放大镜、悬停边框与状态提示，
    /// 因此走局部脏区增量路径；选区、标注、工具栏等任何结构变化都回退到全量重绘。
    ///
    /// # Errors
    ///
    /// 像素缓冲分配、表面调整或呈现失败时返回 [`UiError`]。
    pub fn render_and_present(
        &mut self,
        state: &mut SnipperState,
        bg_image: &RgbaImage,
    ) -> Result<(), UiError> {
        if self.can_incremental(state) {
            self.render_incremental(state, bg_image);
        } else {
            self.render_full(state, bg_image);
        }

        let _ = self.surface.present(&self.render_pixmap)?;
        Ok(())
    }

    /// 返回透明表面延迟提交后的下一次重试时刻。
    pub(crate) fn present_retry_deadline(&self) -> Option<std::time::Instant> {
        self.surface.present_retry_deadline()
    }

    /// 判断本帧能否复用上一帧画面，只重绘光标覆盖层。
    fn can_incremental(&self, state: &SnipperState) -> bool {
        let Some(prev) = &self.overlay else {
            return false;
        };
        matches!(state.viewport.selection, SelectionState::Idle)
            && state.editor.annotation.active_text_input.is_none()
            && prev.hovered_window == state.viewport.hovered_window
            && prev.magnifier_signature == magnifier_signature(&state.viewport.magnifier)
    }

    /// 全量重绘：恢复底图后绘制全部图层，并记录可供下一次增量复用的覆盖层快照。
    fn render_full(&mut self, state: &mut SnipperState, bg_image: &RgbaImage) {
        self.sync_preview_cache(state);
        let mut canvas = Self::build_canvas(
            &self.base_bg,
            state,
            bg_image,
            self.preview_cache.as_ref().map(|cache| &cache.pixmap),
        );
        {
            let mut pixmap_mut = self.render_pixmap.as_mut();
            canvas.render(&mut pixmap_mut);
        }

        self.overlay = Self::capture_overlay(state, &self.render_pixmap);
    }

    /// 增量重绘：抹除上一帧光标覆盖层，重绘悬停边框、放大镜与状态提示，只同步局部像素。
    fn render_incremental(&mut self, state: &mut SnipperState, bg_image: &RgbaImage) {
        let Some(prev) = self.overlay.take() else {
            return;
        };
        let (width, height) = (self.render_pixmap.width(), self.render_pixmap.height());
        self.restore_previous_overlay(&prev, bg_image);
        self.redraw_incremental_overlay(state, bg_image);
        let next = self.next_overlay_snapshot(state, width, height);
        self.overlay = Some(next);
    }

    /// 抹除上一帧覆盖层；悬停窗口内的像素使用原始屏幕图恢复。
    fn restore_previous_overlay(&mut self, previous: &OverlaySnapshot, bg_image: &RgbaImage) {
        if let Some(rect) = previous.magnifier_rect {
            self.restore_covered_region(&rect, previous.hovered_window, bg_image);
        }
        if let Some(rect) = previous.status_rect {
            self.restore_covered_region(&rect, previous.hovered_window, bg_image);
        }
    }

    /// 悬停区域未变化时只重绘边框、放大镜与状态提示。
    fn redraw_incremental_overlay(&mut self, state: &mut SnipperState, bg_image: &RgbaImage) {
        let canvas = Self::build_canvas(&self.base_bg, state, bg_image, None);
        let mut pixmap_mut = self.render_pixmap.as_mut();
        canvas.render_idle_overlay(&mut pixmap_mut, false);
        canvas.render_status_message(&mut pixmap_mut);
    }

    /// 计算本帧覆盖层快照，供下一帧增量恢复。
    fn next_overlay_snapshot(
        &self,
        state: &SnipperState,
        width: u32,
        height: u32,
    ) -> OverlaySnapshot {
        let (cx, cy) = (
            state.viewport.cursor_pos.0 as i32,
            state.viewport.cursor_pos.1 as i32,
        );
        let magnifier_rect = if state.viewport.cursor_ready {
            state.viewport.magnifier.card_bounds(MagnifierViewport {
                cursor_x: cx,
                cursor_y: cy,
                width,
                height,
            })
        } else {
            None
        };
        let status_rect = SnipperCanvas::status_message_bounds(
            state.session.status_message.as_deref(),
            width,
            state.editor.annotation.toolbar.ui_scale,
        );
        OverlaySnapshot {
            hovered_window: state.viewport.hovered_window,
            magnifier_rect,
            status_rect,
            magnifier_signature: magnifier_signature(&state.viewport.magnifier),
        }
    }

    /// 用底图恢复指定覆盖区域：悬停窗口内使用原始屏幕像素，其余使用预计算底图。
    fn restore_covered_region(&mut self, rect: &Rect, hovered: Option<Rect>, bg_image: &RgbaImage) {
        let width = self.render_pixmap.width();
        let height = self.render_pixmap.height();
        let Some((px, py, pw, ph)) = covering_pixel_bounds(rect, width, height) else {
            return;
        };
        let hover_bounds = hovered.and_then(|r| covering_pixel_bounds(&r, width, height));

        let stride = width as usize * 4;
        let base = self.base_bg.data();
        let original = bg_image.as_raw();
        let dest = self.render_pixmap.data_mut();

        for row in 0..ph as usize {
            let y = py as usize + row;
            let start = y * stride + px as usize * 4;
            let len = pw as usize * 4;

            if let (Some(dest_row), Some(base_row)) = (
                dest.get_mut(start..start + len),
                base.get(start..start + len),
            ) {
                dest_row.copy_from_slice(base_row);
            }

            let Some((hx, hy, hw, hh)) = hover_bounds else {
                continue;
            };
            if y < hy as usize || y >= (hy + hh) as usize {
                continue;
            }
            let left = px.max(hx) as usize;
            let right = (px + pw).min(hx + hw) as usize;
            if right <= left {
                continue;
            }
            let offset = y * stride + left * 4;
            let bytes = (right - left) * 4;
            if let (Some(dest_slice), Some(original_slice)) = (
                dest.get_mut(offset..offset + bytes),
                original.get(offset..offset + bytes),
            ) {
                dest_slice.copy_from_slice(original_slice);
            }
        }
    }

    /// 基于当前状态构建画布渲染上下文
    fn build_canvas<'a>(
        base_bg: &'a Pixmap,
        state: &'a mut SnipperState,
        bg_image: &'a RgbaImage,
        recording_preview: Option<&'a Pixmap>,
    ) -> SnipperCanvas<'a> {
        let pending_stroke = state
            .editor
            .annotation
            .toolbar
            .active_tool
            .as_ref()
            .and_then(|tool| {
                if !state.editor.annotation.stroke.is_empty() {
                    Some((
                        tool,
                        state.editor.annotation.stroke.points.as_slice(),
                        state.editor.annotations.next_badge_number(),
                    ))
                } else {
                    None
                }
            });

        let selected = selected_annotation_view(state);

        SnipperCanvas {
            base_bg,
            original_rgba: bg_image,
            selection: &state.viewport.selection,
            cursor_pos: state.viewport.cursor_pos,
            cursor_ready: state.viewport.cursor_ready,
            hovered_window: state.viewport.hovered_window,
            magnifier: &state.viewport.magnifier,
            toolbar: Some(&state.editor.annotation.toolbar),
            annotations: Some(&state.editor.annotations),
            ui_scale: state.editor.annotation.toolbar.ui_scale,
            accent_color: state.session.config.interface.accent_color.to_rgb(),
            pending_stroke,
            active_text_input: state.editor.annotation.active_text_input.as_ref(),
            status_message: state.session.status_message.as_deref(),
            selected_annotation: selected,
            redaction_preview: state.ocr.redaction_preview.as_ref(),
            qr_codes: state.ocr.qr_result.as_deref(),
            ocr_menu: state.ocr.menu.as_ref(),
            ocr_editor: state.ocr.editor.as_ref(),
            recording: state.recording.as_ref(),
            recording_preview,
            long_capture: state.long_capture.as_mut(),
        }
    }

    /// 同步结果页当前预览帧到可绘制的 Tiny-Skia 位图缓存。
    fn sync_preview_cache(&mut self, state: &SnipperState) {
        let Some(session) = state.recording.as_ref() else {
            self.preview_cache = None;
            return;
        };
        let Some(artifact) = session.artifact() else {
            self.preview_cache = None;
            return;
        };
        let Some(frame) = session.preview_frame() else {
            self.preview_cache = None;
            return;
        };
        let artifact_ptr = std::sync::Arc::as_ptr(&artifact.preview_frames) as usize;
        let frame_index = session.preview_frame_index();
        if self.preview_cache.as_ref().is_some_and(|cache| {
            cache.artifact_ptr == artifact_ptr && cache.frame_index == frame_index
        }) {
            return;
        }
        let Ok(decoded) = image::load_from_memory(&frame.data) else {
            tracing::warn!(
                "解码录屏预览帧失败，预览尺寸: {}x{}",
                frame.width,
                frame.height
            );
            self.preview_cache = None;
            return;
        };
        let rgba = decoded.to_rgba8();
        let Some(mut pixmap) = Pixmap::new(rgba.width(), rgba.height()) else {
            self.preview_cache = None;
            return;
        };
        pixmap.data_mut().copy_from_slice(rgba.as_raw());
        self.preview_cache = Some(PreviewFrameCache {
            artifact_ptr,
            frame_index,
            pixmap,
        });
    }

    /// 按当前状态记录光标覆盖层快照；非空闲态返回 None，强制下一帧全量重绘。
    fn capture_overlay(state: &SnipperState, pixmap: &Pixmap) -> Option<OverlaySnapshot> {
        if !matches!(state.viewport.selection, SelectionState::Idle) {
            return None;
        }
        let (width, height) = (pixmap.width(), pixmap.height());
        let (cx, cy) = (
            state.viewport.cursor_pos.0 as i32,
            state.viewport.cursor_pos.1 as i32,
        );

        Some(OverlaySnapshot {
            hovered_window: state.viewport.hovered_window,
            magnifier_rect: if state.viewport.cursor_ready {
                state.viewport.magnifier.card_bounds(MagnifierViewport {
                    cursor_x: cx,
                    cursor_y: cy,
                    width,
                    height,
                })
            } else {
                None
            },
            status_rect: SnipperCanvas::status_message_bounds(
                state.session.status_message.as_deref(),
                width,
                state.editor.annotation.toolbar.ui_scale,
            ),
            magnifier_signature: magnifier_signature(&state.viewport.magnifier),
        })
    }
}

#[cfg(test)]
mod tests;
