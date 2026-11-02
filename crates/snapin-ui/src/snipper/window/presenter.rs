//! 截图全屏窗口呈现器与底层双缓冲光栅化表面管理。

use super::presenter_support::{covering_pixel_bounds, scale_pixmap_to, selected_annotation_view};
use crate::error::UiError;
use crate::snipper::canvas::SnipperCanvas;
use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{Rect, SelectionState};
use crate::snipper::window::events::SnipperState;
use crate::surface::{
    blit_pixmap_to_surface, blit_words_to_surface, swizzle_pixmap_full, swizzle_pixmap_region,
};
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

/// 高速生成定点整数暗化蒙层底图 ((val * 107) >> 8)
///
/// # Errors
///
/// 像素缓冲分配、表面调整或呈现失败时返回 [`UiError`]。
pub fn create_dimmed_pixmap_fast(img: &RgbaImage) -> Result<Pixmap, UiError> {
    let w = img.width();
    let h = img.height();
    let mut dimmed = Pixmap::new(w, h).ok_or(UiError::PixmapAlloc {
        width: w,
        height: h,
    })?;

    let src = img.as_raw();
    let dst_dim = dimmed.data_mut();

    // 内存单遍流转：仅填充定点整数暗化蒙层
    for (d_dim, s) in dst_dim
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(src.as_chunks::<4>().0)
    {
        d_dim[0] = ((s[0] as u16 * 107) >> 8) as u8;
        d_dim[1] = ((s[1] as u16 * 107) >> 8) as u8;
        d_dim[2] = ((s[2] as u16 * 107) >> 8) as u8;
        d_dim[3] = s[3];
    }

    Ok(dimmed)
}

/// 截图全屏窗口呈现器：管理双缓冲帧缓冲区与 softbuffer 表面呈现
pub struct SnipperPresenter {
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    base_bg: Pixmap,
    render_pixmap: Pixmap,
    /// 与 render_pixmap 逐像素对应的 ARGB 提交缓冲；增量帧只更新脏区，再整体提交
    frame_words: Vec<u32>,
    /// 上一帧光标覆盖层快照；为空表示下一帧必须全量重绘
    overlay: Option<OverlaySnapshot>,
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
        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| UiError::Softbuffer {
                message: e.to_string(),
            })?;
        let mut surface =
            softbuffer::Surface::new(&context, window).map_err(|e| UiError::Softbuffer {
                message: e.to_string(),
            })?;

        let w = NonZeroU32::new(width).ok_or_else(|| UiError::Softbuffer {
            message: "无效的屏幕宽度".to_string(),
        })?;
        let h = NonZeroU32::new(height).ok_or_else(|| UiError::Softbuffer {
            message: "无效的屏幕高度".to_string(),
        })?;
        surface.resize(w, h).map_err(|e| UiError::Softbuffer {
            message: format!("初始化帧缓冲尺寸失败: {e}"),
        })?;

        let base_bg = if dimmed {
            create_dimmed_pixmap_fast(bg_image)?
        } else {
            let mut pixmap =
                Pixmap::new(width, height).ok_or(UiError::PixmapAlloc { width, height })?;
            pixmap.data_mut().copy_from_slice(bg_image.as_raw());
            pixmap
        };
        // 映射窗口前先预刷背景底图，避免出现瞬间白屏或未初始化内容；
        // 预刷只是过渡画面，失败时由随后的首次正式渲染兜底，故仅记录调试日志
        if let Err(error) = blit_pixmap_to_surface(&mut surface, &base_bg) {
            tracing::debug!("预刷截图背景底图失败，等待首次正式渲染覆盖: {error}");
        }

        let render_pixmap =
            Pixmap::new(width, height).ok_or(UiError::PixmapAlloc { width, height })?;
        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(UiError::PixmapAlloc { width, height })?;
        let mut frame_words = Vec::new();
        frame_words
            .try_reserve_exact(pixel_count)
            .map_err(|_| UiError::PixmapAlloc { width, height })?;
        frame_words.resize(pixel_count, 0);

        Ok(Self {
            surface,
            base_bg,
            render_pixmap,
            frame_words,
            overlay: None,
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

        self.surface
            .resize(w, h)
            .map_err(|error| UiError::Softbuffer {
                message: format!("调整截图帧缓冲尺寸失败: {error}"),
            })?;

        self.base_bg = scale_pixmap_to(&self.base_bg, width, height)
            .ok_or(UiError::PixmapAlloc { width, height })?;
        *bg_image = image::imageops::resize(
            bg_image,
            width,
            height,
            image::imageops::FilterType::Triangle,
        );
        self.render_pixmap =
            Pixmap::new(width, height).ok_or(UiError::PixmapAlloc { width, height })?;
        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(UiError::PixmapAlloc { width, height })?;
        self.frame_words.clear();
        self.frame_words.resize(pixel_count, 0);
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
        state: &SnipperState,
        bg_image: &RgbaImage,
    ) -> Result<(), UiError> {
        if self.can_incremental(state) {
            self.render_incremental(state, bg_image);
        } else {
            self.render_full(state, bg_image);
        }

        blit_words_to_surface(&mut self.surface, &self.frame_words)
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
    fn render_full(&mut self, state: &SnipperState, bg_image: &RgbaImage) {
        let canvas = Self::build_canvas(&self.base_bg, state, bg_image);
        {
            let mut pixmap_mut = self.render_pixmap.as_mut();
            canvas.render(&mut pixmap_mut);
        }

        swizzle_pixmap_full(&self.render_pixmap, &mut self.frame_words);
        self.overlay = Self::capture_overlay(state, &self.render_pixmap);
    }

    /// 增量重绘：抹除上一帧光标覆盖层，重绘悬停边框、放大镜与状态提示，只同步局部像素。
    fn render_incremental(&mut self, state: &SnipperState, bg_image: &RgbaImage) {
        let Some(prev) = self.overlay.take() else {
            return;
        };
        let (width, height) = (self.render_pixmap.width(), self.render_pixmap.height());
        self.restore_previous_overlay(&prev, bg_image);
        self.redraw_incremental_overlay(state, bg_image);
        let next = self.next_overlay_snapshot(state, width, height);
        self.sync_overlay_regions(&prev, &next, width, height);
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
    fn redraw_incremental_overlay(&mut self, state: &SnipperState, bg_image: &RgbaImage) {
        let canvas = Self::build_canvas(&self.base_bg, state, bg_image);
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
            state.viewport.magnifier.card_bounds(cx, cy, width, height)
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

    /// 将新旧覆盖区域同步到提交缓冲，避免局部像素停留在旧帧。
    fn sync_overlay_regions(
        &mut self,
        previous: &OverlaySnapshot,
        next: &OverlaySnapshot,
        width: u32,
        height: u32,
    ) {
        let dirty = [
            previous.magnifier_rect,
            previous.status_rect,
            next.magnifier_rect,
            next.status_rect,
        ];
        for region in dirty.into_iter().flatten() {
            if let Some(bounds) = covering_pixel_bounds(&region, width, height) {
                swizzle_pixmap_region(&self.render_pixmap, &mut self.frame_words, bounds);
            }
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
        state: &'a SnipperState,
        bg_image: &'a RgbaImage,
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
            selected_annotation: selected_annotation_view(state),
            redaction_preview: state.ocr.redaction_preview.as_ref(),
            qr_codes: state.ocr.qr_result.as_deref(),
            ocr_panel: state.ocr.panel.as_ref(),
        }
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
                state.viewport.magnifier.card_bounds(cx, cy, width, height)
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

    /// 获取截图背景底图引用
    pub fn base_bg(&self) -> &Pixmap {
        &self.base_bg
    }
}

/// 生成放大镜配置签名，用于判断卡片几何或外观是否发生变化。
fn magnifier_signature(magnifier: &Magnifier) -> MagnifierSignature {
    (
        magnifier.enabled,
        magnifier.scale,
        magnifier.show_grid,
        magnifier.ui_scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use tiny_skia::Color;

    #[test]
    fn test_create_dimmed_pixmap_fast_algorithm() {
        let mut img = RgbaImage::new(2, 2);
        img.put_pixel(0, 0, Rgba([255, 255, 255, 255]));
        img.put_pixel(1, 1, Rgba([100, 100, 100, 200]));

        let dimmed = create_dimmed_pixmap_fast(&img).expect("定点暗化生成失败");
        assert_eq!(dimmed.width(), 2);
        assert_eq!(dimmed.height(), 2);

        let data = dimmed.data();
        // 验证纯白像素暗化结果：(255 * 107) >> 8 = 27285 >> 8 = 106
        assert_eq!(data[0], 106);
        assert_eq!(data[1], 106);
        assert_eq!(data[2], 106);
        assert_eq!(data[3], 255);
    }

    /// 脏区包围盒必须向外取整并裁剪到画布范围
    #[test]
    fn test_covering_pixel_bounds() {
        let rect = Rect {
            x: 10.4,
            y: 20.7,
            width: 30.2,
            height: 5.1,
        };
        assert_eq!(
            covering_pixel_bounds(&rect, 100, 100),
            Some((10, 20, 31, 6))
        );

        // 完全越界或零面积返回 None
        assert_eq!(
            covering_pixel_bounds(
                &Rect {
                    x: -50.0,
                    y: -50.0,
                    width: 10.0,
                    height: 10.0,
                },
                100,
                100,
            ),
            None
        );
        assert_eq!(
            covering_pixel_bounds(
                &Rect {
                    x: 99.0,
                    y: 99.0,
                    width: 50.0,
                    height: 50.0,
                },
                100,
                100,
            ),
            Some((99, 99, 1, 1))
        );
    }

    /// 窗口尺寸变化后底图必须匹配新缓冲尺寸，否则渲染会因切片长度不一致 panic。
    #[test]
    fn test_scale_pixmap_to_target_size() {
        let mut source = Pixmap::new(4, 2).expect("创建源位图失败");
        source.fill(Color::from_rgba8(10, 20, 30, 255));
        let scaled = scale_pixmap_to(&source, 8, 4).expect("缩放应成功");
        assert_eq!((scaled.width(), scaled.height()), (8, 4));
    }
}
