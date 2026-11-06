//! 截图呈现器的像素缓冲区与选区辅助函数。

use crate::snipper::canvas::SelectedAnnotation;
use crate::snipper::selection::{Rect, SelectionState};
use crate::snipper::window::events::SnipperState;
use tiny_skia::{Color, FilterQuality, Pixmap, PixmapPaint, Transform};

/// 将底图缩放或裁剪到目标尺寸；在尺寸轻微变化时优先 1:1 物理对齐以保持像素锐利。
pub(super) fn scale_pixmap_to(source: &Pixmap, width: u32, height: u32) -> Option<Pixmap> {
    let mut target = Pixmap::new(width, height)?;
    target.fill(Color::BLACK);
    let dw = (width as i32 - source.width() as i32).abs();
    let dh = (height as i32 - source.height() as i32).abs();

    if dw <= 64 && dh <= 64 {
        target.draw_pixmap(
            0,
            0,
            source.as_ref(),
            &PixmapPaint {
                quality: FilterQuality::Nearest,
                ..PixmapPaint::default()
            },
            Transform::identity(),
            None,
        );
    } else {
        let scale_x = width as f32 / source.width() as f32;
        let scale_y = height as f32 / source.height() as f32;
        target.draw_pixmap(
            0,
            0,
            source.as_ref(),
            &PixmapPaint {
                quality: FilterQuality::Bilinear,
                ..PixmapPaint::default()
            },
            Transform::from_scale(scale_x, scale_y),
            None,
        );
    }
    Some(target)
}

/// 仅在选择器锁定状态下构建选中标注的编辑框视图。
pub(super) fn selected_annotation_view(state: &SnipperState) -> Option<SelectedAnnotation> {
    if !matches!(state.viewport.selection, SelectionState::Locked { .. }) {
        return None;
    }
    let rect = super::annotation_edit::selected_bounds(state)?;
    Some(SelectedAnnotation {
        rect,
        resizable: super::annotation_edit::selected_supports_resize(state),
    })
}

/// 将浮点矩形向外取整为像素包围盒，并裁剪到画布范围。
pub(super) fn covering_pixel_bounds(
    rect: &Rect,
    max_w: u32,
    max_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    let x0 = rect.x.floor().max(0.0);
    let y0 = rect.y.floor().max(0.0);
    let x1 = (rect.x + rect.width).ceil().min(max_w as f32);
    let y1 = (rect.y + rect.height).ceil().min(max_h as f32);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}
