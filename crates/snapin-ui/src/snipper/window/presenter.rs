//! 截图全屏窗口呈现器与底层双缓冲光栅化表面管理。

use crate::error::UiError;
use crate::snipper::canvas::SnipperCanvas;
use crate::snipper::window::events::SnipperState;
use crate::surface::blit_pixmap_to_surface;
use image::RgbaImage;
use std::num::NonZeroU32;
use std::sync::Arc;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 高速生成定点整数暗化蒙层底图 ((val * 107) >> 8)
pub fn create_dimmed_pixmap_fast(img: &RgbaImage) -> Result<Pixmap, UiError> {
    let w = img.width();
    let h = img.height();
    let mut dimmed = Pixmap::new(w, h).ok_or_else(|| UiError::PixmapAlloc {
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
    dimmed_bg: Pixmap,
    render_pixmap: Pixmap,
}

impl SnipperPresenter {
    /// 初始化截图呈现器与底层共享帧缓冲表面
    pub fn try_new(
        window: Arc<Window>,
        width: u32,
        height: u32,
        bg_image: &RgbaImage,
    ) -> Result<Self, UiError> {
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

        let dimmed_bg = create_dimmed_pixmap_fast(bg_image)?;
        // 映射窗口前先预刷暗化蒙层，避免出现瞬间白屏或未初始化内容
        let _ = blit_pixmap_to_surface(&mut surface, &dimmed_bg);

        let render_pixmap =
            Pixmap::new(width, height).ok_or_else(|| UiError::PixmapAlloc { width, height })?;

        Ok(Self {
            surface,
            dimmed_bg,
            render_pixmap,
        })
    }

    /// 执行全屏离屏光栅化并将成果绘制呈现至物理屏幕表面
    pub fn render_and_present(
        &mut self,
        state: &SnipperState,
        bg_image: &RgbaImage,
    ) -> Result<(), UiError> {
        let pending_stroke = state.toolbar.active_tool.as_ref().and_then(|tool| {
            if !state.pending_stroke_points.is_empty() {
                Some((
                    tool,
                    state.pending_stroke_points.as_slice(),
                    state.next_badge_number,
                ))
            } else {
                None
            }
        });

        let canvas = SnipperCanvas {
            dimmed_bg: &self.dimmed_bg,
            original_rgba: bg_image,
            selection: &state.selection,
            cursor_pos: state.cursor_pos,
            magnifier: &state.magnifier,
            toolbar: Some(&state.toolbar),
            annotations: Some(&state.annotations),
            ui_scale: state.toolbar.ui_scale,
            accent_color: state.config.interface.accent_color.to_rgb(),
            pending_stroke,
            active_text_input: state.active_text_input.as_ref(),
            status_message: state.status_message.as_deref(),
        };

        let mut pixmap_mut = self.render_pixmap.as_mut();
        canvas.render(&mut pixmap_mut);

        blit_pixmap_to_surface(&mut self.surface, &self.render_pixmap)
    }

    /// 获取暗化蒙层底图引用
    pub fn dimmed_bg(&self) -> &Pixmap {
        &self.dimmed_bg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

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
}
