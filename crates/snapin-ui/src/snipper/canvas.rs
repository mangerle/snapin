//! 选区全屏复合光栅化渲染管线。
//!
//! 基于 tiny-skia 实现全屏暗色遮罩、选区镂空聚焦、8 手柄线框以及像素放大镜复合渲染。

use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{Rect, SelectionState};
use crate::snipper::toolbar::{Toolbar, ToolbarItem};
use crate::snipper::window::annotation_builder::{render_draft_preview, AnnotationDraftParams};
use crate::snipper::window::text_input::TextInputSession;
use image::RgbaImage;
use snapin_core::annotation::AnnotationHistory;
use snapin_core::render::with_thread_local_text_renderer;
use tiny_skia::{Paint, PathBuilder, Pixmap, PixmapMut, Rect as SkiaRect, Stroke, Transform};

/// 正在拉伸绘制中的待提交标注笔触元数据 (工具类型, 轨迹点集合, 预设色值)
pub type PendingStroke<'a> = (&'a ToolbarItem, &'a [(f32, f32)], u32);

/// 全屏选区画布渲染上下文
pub struct SnipperCanvas<'a> {
    pub base_bg: &'a Pixmap,
    pub original_rgba: &'a RgbaImage,
    pub selection: &'a SelectionState,
    pub cursor_pos: (f64, f64),
    /// 是否已收到首个光标位置；未就绪时隐藏放大镜，避免在左上角绘制占位框
    pub cursor_ready: bool,
    /// 未拉框时鼠标悬停的窗口矩形，用于窗口截图预览
    pub hovered_window: Option<Rect>,
    pub magnifier: &'a Magnifier,
    pub toolbar: Option<&'a Toolbar>,
    pub annotations: Option<&'a AnnotationHistory>,
    pub ui_scale: f32,
    pub accent_color: (u8, u8, u8),
    pub pending_stroke: Option<PendingStroke<'a>>,
    pub active_text_input: Option<&'a TextInputSession>,
    pub status_message: Option<&'a str>,
}

impl<'a> SnipperCanvas<'a> {
    /// 执行完整帧的高性能复合渲染并写入输出 pixmap
    pub fn render(&self, pixmap: &mut PixmapMut) {
        // 1. 高速整块恢复预计算好的背景底图（普通截图为暗化图，取色器为原图）
        pixmap.data_mut().copy_from_slice(self.base_bg.data());

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

                // 实时渲染当前活跃编辑中的文本会话
                if let Some(text_session) = self.active_text_input {
                    text_session.render(pixmap);
                }

                // 实时渲染当前正在拖拽拉伸中的标注图元即时预览 (光标移动即刻变化)
                if let (Some(tb), Some((tool, pts, badge_num))) =
                    (self.toolbar, self.pending_stroke)
                {
                    self.render_pending_preview(pixmap, tb, tool, pts, badge_num);
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
            // 未收到首个光标位置前不绘制放大镜：winit 刚映射窗口时没有光标坐标，
            // 若直接用 (0,0) 会在左上角留下一个误导性的像素框。
            if self.cursor_ready {
                if let Some(hover) = self.hovered_window {
                    self.fast_cutout_restore(pixmap, &hover, screen_w, screen_h);
                    self.render_window_hover_border(pixmap, &hover);
                }
                self.magnifier.render(
                    pixmap,
                    self.original_rgba,
                    self.cursor_pos.0 as i32,
                    self.cursor_pos.1 as i32,
                );
            }
        }

        self.render_status_message(pixmap);
    }

    /// 在画布顶部居中绘制操作失败或空结果提示
    fn render_status_message(&self, pixmap: &mut PixmapMut) {
        let Some(message) = self.status_message else {
            return;
        };
        if message.is_empty() {
            return;
        }

        let scale = self.ui_scale.max(0.5);
        let font_size = 13.0 * scale;
        let padding_x = 14.0 * scale;
        let padding_y = 8.0 * scale;
        let estimated_width = message
            .chars()
            .map(|ch| if ch.is_ascii() { 7.5 } else { 13.0 })
            .sum::<f32>()
            * scale;
        let max_width = (pixmap.width() as f32 - 16.0 * scale).max(120.0 * scale);
        let box_width = (estimated_width + padding_x * 2.0).min(max_width);
        let box_height = font_size * 1.4 + padding_y * 2.0;
        let box_x = ((pixmap.width() as f32 - box_width) / 2.0).max(8.0 * scale);
        let box_y = 16.0 * scale;

        let mut path = PathBuilder::new();
        let radius = 6.0 * scale;
        path.move_to(box_x + radius, box_y);
        path.line_to(box_x + box_width - radius, box_y);
        path.quad_to(box_x + box_width, box_y, box_x + box_width, box_y + radius);
        path.line_to(box_x + box_width, box_y + box_height - radius);
        path.quad_to(
            box_x + box_width,
            box_y + box_height,
            box_x + box_width - radius,
            box_y + box_height,
        );
        path.line_to(box_x + radius, box_y + box_height);
        path.quad_to(
            box_x,
            box_y + box_height,
            box_x,
            box_y + box_height - radius,
        );
        path.line_to(box_x, box_y + radius);
        path.quad_to(box_x, box_y, box_x + radius, box_y);
        path.close();

        if let Some(path) = path.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(28, 30, 38, 238);
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
            let mut border = Paint::default();
            border.set_color_rgba8(255, 255, 255, 36);
            let stroke = Stroke {
                width: 1.0 * scale,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &border, &stroke, Transform::identity(), None);
        }

        with_thread_local_text_renderer(|renderer| {
            renderer.draw_text(
                pixmap,
                message,
                box_x + padding_x,
                box_y + padding_y,
                font_size,
                tiny_skia::Color::from_rgba8(240, 243, 250, 255),
            );
        });
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
        let orig_data = self.original_rgba.as_raw();
        let dest_data = pixmap.data_mut();

        for row in 0..ph {
            let y_idx = (py + row) as usize;
            let start_offset = y_idx * stride + (px as usize) * 4;
            let end_offset = start_offset + row_bytes;
            dest_data[start_offset..end_offset]
                .copy_from_slice(&orig_data[start_offset..end_offset]);
        }
    }

    /// 绘制窗口截图悬停高亮：只画主题色边框，不显示缩放锚点
    fn render_window_hover_border(&self, pixmap: &mut PixmapMut, rect: &Rect) {
        let Some(skia_rect) = SkiaRect::from_xywh(rect.x, rect.y, rect.width, rect.height) else {
            return;
        };
        let s = self.ui_scale;
        let (ar, ag, ab) = self.accent_color;
        let mut paint = Paint::default();
        paint.set_color_rgba8(ar, ag, ab, 255);
        let stroke = Stroke {
            width: 2.0 * s,
            ..Default::default()
        };
        let mut pb = PathBuilder::new();
        pb.push_rect(skia_rect);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }

    /// 绘制高亮线框与 8 个缩放手柄 (自适应 HiDPI 缩放)
    fn render_selection_borders_and_handles(&self, pixmap: &mut PixmapMut, rect: &Rect) {
        let s = self.ui_scale;
        if let Some(skia_r) = SkiaRect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
            let (ar, ag, ab) = self.accent_color;
            let mut stroke_paint = Paint::default();
            stroke_paint.set_color_rgba8(ar, ag, ab, 255);
            let stroke = Stroke {
                width: 2.0 * s,
                ..Default::default()
            };

            let mut pb = PathBuilder::new();
            pb.push_rect(skia_r);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
            }

            // 绘制 8 个控制手柄 (4个角点 + 4条边中点，白色方块带主题描边)
            let handle_size = (6.0 * s).round().max(4.0);
            let handles = [
                (rect.x, rect.y),
                (rect.x + rect.width / 2.0, rect.y),
                (rect.x + rect.width, rect.y),
                (rect.x + rect.width, rect.y + rect.height / 2.0),
                (rect.x + rect.width, rect.y + rect.height),
                (rect.x + rect.width / 2.0, rect.y + rect.height),
                (rect.x, rect.y + rect.height),
                (rect.x, rect.y + rect.height / 2.0),
            ];

            let mut handle_paint = Paint::default();
            handle_paint.set_color_rgba8(255, 255, 255, 255);
            let mut handle_border_paint = Paint::default();
            handle_border_paint.set_color_rgba8(ar, ag, ab, 255);
            let handle_stroke = Stroke {
                width: 1.0 * s,
                ..Default::default()
            };

            for (cx, cy) in handles {
                if let Some(hr) = SkiaRect::from_xywh(
                    cx - handle_size / 2.0,
                    cy - handle_size / 2.0,
                    handle_size,
                    handle_size,
                ) {
                    pixmap.fill_rect(hr, &handle_paint, Transform::identity(), None);
                    let mut b_pb = PathBuilder::new();
                    b_pb.push_rect(hr);
                    if let Some(bp) = b_pb.finish() {
                        pixmap.stroke_path(
                            &bp,
                            &handle_border_paint,
                            &handle_stroke,
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
        }
    }

    /// 实时渲染当前正在交互拖拽中的即时图元 (支持矩形、椭圆、箭头、画笔、记号笔、马赛克、序号实时动态伸缩)
    fn render_pending_preview(
        &self,
        pixmap: &mut PixmapMut,
        toolbar: &Toolbar,
        tool: &ToolbarItem,
        pts: &[(f32, f32)],
        badge_num: u32,
    ) {
        if let Some(params) =
            AnnotationDraftParams::from_toolbar_and_points(toolbar, *tool, pts, badge_num)
        {
            render_draft_preview(&params, pixmap);
        }
    }
}
