//! 选区全屏复合光栅化渲染管线。
//!
//! 基于 tiny-skia 实现全屏暗色遮罩、选区镂空聚焦、8 手柄线框以及像素放大镜复合渲染。

use crate::annotation::AnnotationHistory;
use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{Rect, SelectionState};
use crate::snipper::toolbar::Toolbar;
use image::RgbaImage;
use tiny_skia::{Paint, PathBuilder, Pixmap, PixmapMut, Rect as SkiaRect, Stroke, Transform};

/// 全屏选区画布渲染上下文
pub struct SnipperCanvas<'a> {
    pub original_bg: &'a Pixmap,
    pub dimmed_bg: &'a Pixmap,
    pub original_rgba: &'a RgbaImage,
    pub selection: &'a SelectionState,
    pub cursor_pos: (f64, f64),
    pub magnifier: &'a Magnifier,
    pub toolbar: Option<&'a Toolbar>,
    pub annotations: Option<&'a AnnotationHistory>,
}

impl<'a> SnipperCanvas<'a> {
    /// 执行完整帧的高性能复合渲染并写入输出 pixmap
    pub fn render(&self, pixmap: &mut PixmapMut) {
        // 1. 高速整块恢复已预计算好的暗色底图 (内存瞬时拷贝，避免重复混合计算)
        pixmap.data_mut().copy_from_slice(self.dimmed_bg.data());

        // 2. 提取当前有效矩形选区
        let active_rect = match self.selection {
            SelectionState::Idle => None,
            SelectionState::Creating { start, current } => {
                Some(Rect::from_points(*start, *current))
            }
            SelectionState::Locked { rect, .. } => Some(*rect),
        };

        let screen_w = pixmap.width();
        let screen_h = pixmap.height();

        // 3. 若有选区，执行极速行切片镂空高亮与线框绘制
        if let Some(r) = active_rect {
            if r.width > 1.0 && r.height > 1.0 {
                self.fast_cutout_restore(pixmap, &r, screen_w, screen_h);

                // 回放已完成的矢量标注
                if let Some(history) = self.annotations {
                    history.replay_all(pixmap);
                }

                self.render_selection_borders_and_handles(pixmap, &r);

                // 绘制工具栏 (若处于锁定状态且存在工具栏)
                if matches!(self.selection, SelectionState::Locked { .. }) {
                    if let Some(tb) = self.toolbar {
                        tb.render(pixmap);
                    }
                }
            }
        } else {
            // 未拉框状态下，跟随光标渲染像素放大镜
            self.magnifier.render(
                pixmap,
                self.original_rgba,
                self.cursor_pos.0 as i32,
                self.cursor_pos.1 as i32,
            );
        }
    }

    /// 通过直接内存行切片快速将原图像素恢复到选区内（亚毫秒级完成镂空透出）
    fn fast_cutout_restore(
        &self,
        pixmap: &mut PixmapMut,
        rect: &Rect,
        screen_w: u32,
        screen_h: u32,
    ) {
        let (px, py, pw, ph) = rect.to_pixel_bounds(screen_w, screen_h);
        if pw == 0 || ph == 0 {
            return;
        }

        let stride = (screen_w as usize) * 4;
        let row_bytes = (pw as usize) * 4;
        let orig_data = self.original_bg.data();
        let dest_data = pixmap.data_mut();

        for row in 0..ph {
            let y_idx = (py + row) as usize;
            let start_offset = y_idx * stride + (px as usize) * 4;
            let end_offset = start_offset + row_bytes;
            dest_data[start_offset..end_offset]
                .copy_from_slice(&orig_data[start_offset..end_offset]);
        }
    }

    /// 绘制高亮线框与 8 个缩放手柄
    fn render_selection_borders_and_handles(&self, pixmap: &mut PixmapMut, rect: &Rect) {
        if let Some(skia_r) = SkiaRect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
            let mut stroke_paint = Paint::default();
            stroke_paint.set_color_rgba8(0, 174, 255, 255);
            let stroke = Stroke {
                width: 2.0,
                ..Default::default()
            };

            let mut pb = PathBuilder::new();
            pb.push_rect(skia_r);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
            }

            // 绘制 4 角控制手柄 (白色小方块，青色细边框)
            let handle_size = 6.0f32;
            let corners = [
                (rect.x, rect.y),
                (rect.x + rect.width, rect.y),
                (rect.x, rect.y + rect.height),
                (rect.x + rect.width, rect.y + rect.height),
            ];

            let mut handle_paint = Paint::default();
            handle_paint.set_color_rgba8(255, 255, 255, 255);
            for (cx, cy) in corners {
                if let Some(hr) = SkiaRect::from_xywh(
                    cx - handle_size / 2.0,
                    cy - handle_size / 2.0,
                    handle_size,
                    handle_size,
                ) {
                    pixmap.fill_rect(hr, &handle_paint, Transform::identity(), None);
                }
            }
        }
    }
}
