//! 选区全屏复合光栅化渲染管线。
//!
//! 基于 tiny-skia 实现全屏暗色遮罩、选区镂空聚焦、8 手柄线框以及像素放大镜复合渲染。

use crate::annotation::AnnotationHistory;
use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{Rect, SelectionState};
use crate::snipper::toolbar::Toolbar;
use image::RgbaImage;
use tiny_skia::{
    Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Rect as SkiaRect, Stroke, Transform,
};

/// 全屏选区画布渲染上下文
pub struct SnipperCanvas<'a> {
    pub original_bg: &'a Pixmap,
    pub original_rgba: &'a RgbaImage,
    pub selection: &'a SelectionState,
    pub cursor_pos: (f64, f64),
    pub magnifier: &'a Magnifier,
    pub toolbar: Option<&'a Toolbar>,
    pub annotations: Option<&'a AnnotationHistory>,
}

impl<'a> SnipperCanvas<'a> {
    /// 执行完整帧的光栅化复合渲染并写入输出 pixmap
    pub fn render(&self, pixmap: &mut PixmapMut) {
        // 1. 铺设原始全屏底图
        pixmap.draw_pixmap(
            0,
            0,
            self.original_bg.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );

        // 2. 覆盖半透明暗色遮罩
        let screen_w = pixmap.width() as f32;
        let screen_h = pixmap.height() as f32;
        if let Some(screen_rect) = SkiaRect::from_xywh(0.0, 0.0, screen_w, screen_h) {
            let mut mask_paint = Paint::default();
            mask_paint.set_color_rgba8(0, 0, 0, 110);
            pixmap.fill_rect(screen_rect, &mask_paint, Transform::identity(), None);
        }

        // 3. 提取当前有效矩形选区
        let active_rect = match self.selection {
            SelectionState::Idle => None,
            SelectionState::Creating { start, current } => {
                Some(Rect::from_points(*start, *current))
            }
            SelectionState::Locked { rect, .. } => Some(*rect),
        };

        // 4. 若有选区，执行镂空高亮与线框绘制
        if let Some(r) = active_rect {
            if r.width > 1.0 && r.height > 1.0 {
                self.render_cutout(pixmap, &r);
                // 回放已完成的标注操作
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
            // 未拉框状态下，跟随光标渲染放大镜
            self.magnifier.render(
                pixmap,
                self.original_rgba,
                self.cursor_pos.0 as i32,
                self.cursor_pos.1 as i32,
            );
        }
    }

    /// 选区镂空恢复原图
    fn render_cutout(&self, pixmap: &mut PixmapMut, rect: &Rect) {
        if let Some(skia_r) = SkiaRect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
            // 清除暗色蒙版，原底图透出
            let clear_paint = Paint {
                blend_mode: tiny_skia::BlendMode::Clear,
                ..Default::default()
            };
            pixmap.fill_rect(skia_r, &clear_paint, Transform::identity(), None);

            // 重新填入选区对应的原图
            pixmap.draw_pixmap(
                0,
                0,
                self.original_bg.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
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
