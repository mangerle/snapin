//! 选区全屏复合光栅化渲染管线。
//!
//! 基于 tiny-skia 实现全屏暗色遮罩、选区镂空聚焦、8 手柄线框以及像素放大镜复合渲染。

use crate::editor::text_input::TextInputSession;
use crate::editor::toolbar::{Tool, Toolbar};
use crate::ports::QrCodeItem;
use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{Rect, SelectionState};
use crate::snipper::window::annotation_builder::{render_draft_preview, AnnotationDraftParams};
use crate::snipper::window::ocr_panel::OcrPanelState;
use crate::snipper::window::redaction::RedactionPreview;
use crate::status_badge;
use image::RgbaImage;
use snapin_core::annotation::AnnotationHistory;
use snapin_core::render::{with_thread_local_text_renderer, TextBoxOptions};
use tiny_skia::{
    Paint, PathBuilder, Pixmap, PixmapMut, Rect as SkiaRect, Stroke, StrokeDash, Transform,
};

/// 正在拉伸绘制中的待提交标注笔触元数据 (工具类型, 轨迹点集合, 预设色值)
pub type PendingStroke<'a> = (&'a Tool, &'a [(f32, f32)], u32);

/// 选中标注的编辑框信息
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectedAnnotation {
    /// 标注的轴对齐包围盒
    pub rect: Rect,
    /// 是否允许通过手柄拉伸（原位翻译块只允许整体移动）
    pub resizable: bool,
}

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
    /// 当前选中的已提交标注编辑框
    pub selected_annotation: Option<SelectedAnnotation>,
    /// 智能打码待确认区域
    pub redaction_preview: Option<&'a RedactionPreview>,
    /// 最近一次二维码识别结果的包围盒
    pub qr_codes: Option<&'a [QrCodeItem]>,
    /// OCR 结果面板
    pub ocr_panel: Option<&'a OcrPanelState>,
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

                if let Some(panel) = self.ocr_panel {
                    self.render_ocr_panel(pixmap, panel);
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

                self.render_detection_previews(pixmap);

                self.render_selection_borders_and_handles(pixmap, &r);

                if let Some(selected) = self.selected_annotation {
                    self.render_annotation_selection(pixmap, &selected);
                }

                // 绘制工具栏 (若处于锁定状态且存在工具栏)
                if matches!(self.selection, SelectionState::Locked { .. }) {
                    if let Some(tb) = self.toolbar {
                        tb.render(pixmap);
                    }
                }
            }
        } else {
            self.render_idle_overlay(pixmap, true);
        }

        self.render_status_message(pixmap);
    }

    /// 绘制 OCR 结果面板与可见文本行。
    fn render_ocr_panel(&self, pixmap: &mut PixmapMut, panel: &OcrPanelState) {
        let s = self.ui_scale;
        let layout = panel.layout(pixmap.width() as f32, pixmap.height() as f32, s);
        let mut background = Paint::default();
        background.set_color_rgba8(15, 23, 42, 242);
        if let Some(rect) = SkiaRect::from_xywh(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width,
            layout.panel.height,
        ) {
            pixmap.fill_rect(rect, &background, Transform::identity(), None);
        }

        let mut border = Paint::default();
        border.set_color_rgba8(71, 85, 105, 255);
        let border_stroke = Stroke {
            width: s,
            ..Default::default()
        };
        let mut panel_pb = PathBuilder::new();
        if let Some(rect) = SkiaRect::from_xywh(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width,
            layout.panel.height,
        ) {
            panel_pb.push_rect(rect);
            if let Some(path) = panel_pb.finish() {
                pixmap.stroke_path(&path, &border, &border_stroke, Transform::identity(), None);
            }
        }

        with_thread_local_text_renderer(|renderer| {
            renderer.draw_text_rgba(
                pixmap,
                &format!(
                    "OCR 结果  已选择 {}/{}",
                    panel.lines.iter().filter(|line| line.selected).count(),
                    panel.lines.len()
                ),
                layout.panel.x + 14.0 * s,
                layout.panel.y + 12.0 * s,
                16.0 * s,
                (241, 245, 249, 255),
            );

            for (line_index, row) in &layout.rows {
                if Some(*line_index) == Some(panel.cursor) {
                    let mut focus = Paint::default();
                    focus.set_color_rgba8(37, 99, 235, 96);
                    if let Some(rect) = SkiaRect::from_xywh(
                        row.x,
                        row.y,
                        row.width,
                        (row.height - 2.0 * s).max(1.0),
                    ) {
                        pixmap.fill_rect(rect, &focus, Transform::identity(), None);
                    }
                }
                let Some(line) = panel.lines.get(*line_index) else {
                    continue;
                };
                let box_size = 12.0 * s;
                let box_x = row.x + 7.0 * s;
                let box_y = row.y + (row.height - box_size) / 2.0;
                let mut box_paint = Paint::default();
                box_paint.set_color_rgba8(241, 245, 249, 255);
                if let Some(rect) = SkiaRect::from_xywh(box_x, box_y, box_size, box_size) {
                    pixmap.fill_rect(rect, &box_paint, Transform::identity(), None);
                    if line.selected {
                        let mut check = PathBuilder::new();
                        check.move_to(box_x + 2.5 * s, box_y + 6.0 * s);
                        check.line_to(box_x + 5.0 * s, box_y + 9.0 * s);
                        check.line_to(box_x + 10.0 * s, box_y + 3.0 * s);
                        if let Some(path) = check.finish() {
                            let mut check_paint = Paint::default();
                            check_paint.set_color_rgba8(37, 99, 235, 255);
                            let check_stroke = Stroke {
                                width: 2.0 * s,
                                ..Default::default()
                            };
                            pixmap.stroke_path(
                                &path,
                                &check_paint,
                                &check_stroke,
                                Transform::identity(),
                                None,
                            );
                        }
                    }
                }
                renderer.draw_text_box_rgba(
                    pixmap,
                    &line.text,
                    TextBoxOptions {
                        rect: (
                            row.x + 26.0 * s,
                            row.y + 2.0 * s,
                            (row.width - 32.0 * s).max(1.0),
                            (row.height - 3.0 * s).max(1.0),
                        ),
                        font_size: 13.0 * s,
                        rgba: (226, 232, 240, 255),
                    },
                );
            }

            renderer.draw_text_box_rgba(
                pixmap,
                "Space 选择  Enter 编辑  A 全选  M 导出 Markdown  J 导出 JSON  Esc 关闭",
                TextBoxOptions {
                    rect: (
                        layout.panel.x + 14.0 * s,
                        layout.panel.y + layout.panel.height - 38.0 * s,
                        (layout.panel.width - 28.0 * s).max(1.0),
                        28.0 * s,
                    ),
                    font_size: 11.0 * s,
                    rgba: (148, 163, 184, 255),
                },
            );
        });
    }

    /// 绘制智能打码与二维码识别预览框。
    ///
    /// 预览不写入标注历史，只有智能打码确认后才生成真实遮挡图元；二维码框仅用于
    /// 告诉用户系统发现了哪些区域，不改变最终截图内容。
    fn render_detection_previews(&self, pixmap: &mut PixmapMut) {
        let s = self.ui_scale;
        if let Some(preview) = self.redaction_preview {
            let (r, g, b) = self.accent_color;
            let mut fill = Paint::default();
            fill.set_color_rgba8(r, g, b, 36);
            let mut stroke_paint = Paint::default();
            stroke_paint.set_color_rgba8(r, g, b, 255);
            let stroke = Stroke {
                width: (1.5 * s).max(1.0),
                dash: StrokeDash::new(vec![6.0 * s, 4.0 * s], 0.0),
                ..Default::default()
            };
            for rect in &preview.regions {
                if let Some(skia_rect) =
                    SkiaRect::from_xywh(rect.x, rect.y, rect.width, rect.height)
                {
                    pixmap.fill_rect(skia_rect, &fill, Transform::identity(), None);
                    let mut pb = PathBuilder::new();
                    pb.push_rect(skia_rect);
                    if let Some(path) = pb.finish() {
                        pixmap.stroke_path(
                            &path,
                            &stroke_paint,
                            &stroke,
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
        }

        if let Some(codes) = self.qr_codes {
            let mut paint = Paint::default();
            paint.set_color_rgba8(20, 184, 166, 255);
            let stroke = Stroke {
                width: (1.8 * s).max(1.0),
                ..Default::default()
            };
            for code in codes {
                let (x, y, width, height) = code.bounds;
                if let Some(rect) =
                    SkiaRect::from_xywh(x as f32, y as f32, width as f32, height as f32)
                {
                    let mut pb = PathBuilder::new();
                    pb.push_rect(rect);
                    if let Some(path) = pb.finish() {
                        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                    }
                }
            }
        }
    }

    /// 绘制空闲态的光标覆盖层：悬停窗口高亮与像素放大镜。
    ///
    /// `restore_hover` 为 true 时先恢复悬停窗口的原始屏幕像素（全量渲染路径）；
    /// 增量重绘传 false，悬停区域保持不变，避免每帧重复恢复整块窗口。
    pub fn render_idle_overlay(&self, pixmap: &mut PixmapMut, restore_hover: bool) {
        // 未收到首个光标位置前不绘制放大镜：winit 刚映射窗口时没有光标坐标，
        // 若直接用 (0,0) 会在左上角留下一个误导性的像素框。
        if !self.cursor_ready {
            return;
        }

        if let Some(hover) = self.hovered_window {
            if restore_hover {
                self.fast_cutout_restore(pixmap, &hover, pixmap.width(), pixmap.height());
            }
            self.render_window_hover_border(pixmap, &hover);
        }

        self.magnifier.render(
            pixmap,
            self.original_rgba,
            self.cursor_pos.0 as i32,
            self.cursor_pos.1 as i32,
        );
    }

    /// 返回顶部状态提示框覆盖的像素矩形（无消息或空文本时返回 None）。
    ///
    /// 与 [`Self::render_status_message`] 共用同一布局函数，供增量重绘精确抹除旧提示。
    pub fn status_message_bounds(
        message: Option<&str>,
        canvas_w: u32,
        ui_scale: f32,
    ) -> Option<Rect> {
        let message = message?;
        if message.is_empty() {
            return None;
        }
        let layout = status_badge::layout(
            message,
            canvas_w as f32,
            ui_scale,
            status_badge::SNIPPER_STATUS_STYLE,
        );
        Some(status_badge::dirty_rect(&layout))
    }

    /// 在画布顶部居中绘制操作失败或空结果提示
    pub(crate) fn render_status_message(&self, pixmap: &mut PixmapMut) {
        let Some(message) = self.status_message else {
            return;
        };
        if message.is_empty() {
            return;
        }

        let layout = status_badge::layout(
            message,
            pixmap.width() as f32,
            self.ui_scale,
            status_badge::SNIPPER_STATUS_STYLE,
        );
        status_badge::render(pixmap, message, &layout, status_badge::SNIPPER_STATUS_STYLE);
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
            let handles = handle_positions(rect);

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
        tool: &Tool,
        pts: &[(f32, f32)],
        badge_num: u32,
    ) {
        if let Some(params) =
            AnnotationDraftParams::from_toolbar_and_points(toolbar, *tool, pts, badge_num)
        {
            render_draft_preview(&params, pixmap);
        }
    }

    /// 绘制已选中标注的编辑框：白色虚线包围盒与 8 个缩放手柄。
    ///
    /// 虚线样式用于与选区实线边框区分；不可拉伸的复合图元只绘制包围盒。
    fn render_annotation_selection(&self, pixmap: &mut PixmapMut, selected: &SelectedAnnotation) {
        let Some(skia_rect) = SkiaRect::from_xywh(
            selected.rect.x,
            selected.rect.y,
            selected.rect.width,
            selected.rect.height,
        ) else {
            return;
        };
        let s = self.ui_scale;

        let mut dash_paint = Paint::default();
        dash_paint.set_color_rgba8(255, 255, 255, 235);
        let dash = Stroke {
            width: (1.2 * s).max(1.0),
            dash: StrokeDash::new(vec![6.0 * s, 4.0 * s], 0.0),
            ..Default::default()
        };
        let mut pb = PathBuilder::new();
        pb.push_rect(skia_rect);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &dash_paint, &dash, Transform::identity(), None);
        }

        if !selected.resizable {
            return;
        }

        let handle_size = (6.0 * s).round().max(4.0);
        let (ar, ag, ab) = self.accent_color;
        let mut handle_paint = Paint::default();
        handle_paint.set_color_rgba8(255, 255, 255, 255);
        let mut handle_border_paint = Paint::default();
        handle_border_paint.set_color_rgba8(ar, ag, ab, 255);
        let handle_stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };

        for (cx, cy) in handle_positions(&selected.rect) {
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

/// 计算包围盒 8 个控制手柄的中心坐标 (4 个角点 + 4 条边中点)
fn handle_positions(rect: &Rect) -> [(f32, f32); 8] {
    [
        (rect.x, rect.y),
        (rect.x + rect.width / 2.0, rect.y),
        (rect.x + rect.width, rect.y),
        (rect.x + rect.width, rect.y + rect.height / 2.0),
        (rect.x + rect.width, rect.y + rect.height),
        (rect.x + rect.width / 2.0, rect.y + rect.height),
        (rect.x, rect.y + rect.height),
        (rect.x, rect.y + rect.height / 2.0),
    ]
}
