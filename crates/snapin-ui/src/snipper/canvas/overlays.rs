//! 截图覆盖层的光标、检测预览、OCR 面板与状态提示绘制。

use super::*;
use crate::snipper::window::ocr_panel::{OcrPanelLayout, OcrPanelLine};
use snapin_render::TextRenderer;

impl<'a> SnipperCanvas<'a> {
    /// 绘制 OCR 结果面板与可见文本行。
    pub(super) fn render_ocr_panel(&self, pixmap: &mut PixmapMut, panel: &OcrPanelState) {
        let s = self.ui_scale;
        let layout = panel.layout(pixmap.width() as f32, pixmap.height() as f32, s);
        Self::render_ocr_panel_chrome(pixmap, &layout.panel, s);
        self.render_ocr_rows(pixmap, panel, &layout, s);
        Self::render_ocr_footer(pixmap, &layout.panel, s);
    }

    /// 绘制 OCR 面板背景与边框。
    fn render_ocr_panel_chrome(pixmap: &mut PixmapMut, panel: &Rect, s: f32) {
        let mut background = Paint::default();
        background.set_color_rgba8(15, 23, 42, 242);
        if let Some(rect) = SkiaRect::from_xywh(panel.x, panel.y, panel.width, panel.height) {
            pixmap.fill_rect(rect, &background, Transform::identity(), None);
        }

        let mut border = Paint::default();
        border.set_color_rgba8(71, 85, 105, 255);
        let border_stroke = Stroke {
            width: s,
            ..Default::default()
        };
        let mut panel_pb = PathBuilder::new();
        if let Some(rect) = SkiaRect::from_xywh(panel.x, panel.y, panel.width, panel.height) {
            panel_pb.push_rect(rect);
            if let Some(path) = panel_pb.finish() {
                pixmap.stroke_path(&path, &border, &border_stroke, Transform::identity(), None);
            }
        }
    }

    /// 绘制选中数量、可见行与行焦点框。
    fn render_ocr_rows(
        &self,
        pixmap: &mut PixmapMut,
        panel: &OcrPanelState,
        layout: &OcrPanelLayout,
        s: f32,
    ) {
        with_thread_local_text_renderer(|renderer| {
            renderer.draw_text_rgba(
                pixmap,
                &snapin_foundation::trf!(
                    "OCR 结果  已选择 {}/{}",
                    "OCR Results  Selected {}/{}",
                    panel.lines.iter().filter(|line| line.selected).count(),
                    panel.lines.len()
                ),
                layout.panel.x + 14.0 * s,
                layout.panel.y + 12.0 * s,
                16.0 * s,
                (241, 245, 249, 255),
            );

            for (line_index, row) in &layout.rows {
                let Some(line) = panel.lines.get(*line_index) else {
                    continue;
                };
                Self::render_ocr_row(pixmap, renderer, row, line, *line_index == panel.cursor, s);
            }
        });
    }

    /// 绘制单行焦点、复选框和文本。
    fn render_ocr_row(
        pixmap: &mut PixmapMut,
        renderer: &mut TextRenderer,
        row: &Rect,
        line: &OcrPanelLine,
        focused: bool,
        s: f32,
    ) {
        if focused {
            let mut focus = Paint::default();
            focus.set_color_rgba8(37, 99, 235, 96);
            if let Some(rect) =
                SkiaRect::from_xywh(row.x, row.y, row.width, (row.height - 2.0 * s).max(1.0))
            {
                pixmap.fill_rect(rect, &focus, Transform::identity(), None);
            }
        }
        Self::render_ocr_checkbox(pixmap, row, line.selected, s);
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

    /// 绘制 OCR 面板底部快捷键说明。
    fn render_ocr_footer(pixmap: &mut PixmapMut, panel: &Rect, s: f32) {
        with_thread_local_text_renderer(|renderer| {
            renderer.draw_text_box_rgba(
                pixmap,
                snapin_foundation::i18n::tr(
                    "Space 选择  Enter 编辑  A 全选  M 导出 Markdown  J 导出 JSON  Esc 关闭",
                ),
                TextBoxOptions {
                    rect: (
                        panel.x + 14.0 * s,
                        panel.y + panel.height - 38.0 * s,
                        (panel.width - 28.0 * s).max(1.0),
                        28.0 * s,
                    ),
                    font_size: 11.0 * s,
                    rgba: (148, 163, 184, 255),
                },
            );
        });
    }

    /// 绘制智能打码区域边界与二维码识别预览框。
    ///
    /// 智能打码在 OCR 完成时已经把真实马赛克写入标注历史，这里只绘制可点击区域边界，
    /// 不再叠加半透明色块，确保用户看到的就是最终马赛克效果；二维码框仅用于提示区域。
    pub(super) fn render_detection_previews(&self, pixmap: &mut PixmapMut) {
        self.render_redaction_previews(pixmap);
        self.render_qr_previews(pixmap);
    }

    /// 绘制智能打码区域边界；已应用区域使用主题色，未应用区域使用灰色虚线。
    fn render_redaction_previews(&self, pixmap: &mut PixmapMut) {
        let s = self.ui_scale;
        if let Some(preview) = self.redaction_preview {
            let (r, g, b) = self.accent_color;
            let mut stroke_paint = Paint::default();
            stroke_paint.set_color_rgba8(r, g, b, 255);
            let stroke = Stroke {
                width: (1.5 * s).max(1.0),
                dash: StrokeDash::new(vec![6.0 * s, 4.0 * s], 0.0),
                ..Default::default()
            };
            let mut excluded_stroke_paint = Paint::default();
            excluded_stroke_paint.set_color_rgba8(148, 163, 184, 230);
            let excluded_stroke = Stroke {
                width: (1.2 * s).max(1.0),
                dash: StrokeDash::new(vec![3.0 * s, 3.0 * s], 0.0),
                ..Default::default()
            };
            for rect in &preview.regions {
                if let Some(skia_rect) =
                    SkiaRect::from_xywh(rect.x, rect.y, rect.width, rect.height)
                {
                    let applied = self
                        .annotations
                        .is_some_and(|annotations| is_redaction_applied(annotations, *rect));
                    let mut pb = PathBuilder::new();
                    pb.push_rect(skia_rect);
                    if let Some(path) = pb.finish() {
                        pixmap.stroke_path(
                            &path,
                            if applied {
                                &stroke_paint
                            } else {
                                &excluded_stroke_paint
                            },
                            if applied { &stroke } else { &excluded_stroke },
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
        }
    }

    /// 绘制二维码识别边界。
    fn render_qr_previews(&self, pixmap: &mut PixmapMut) {
        let s = self.ui_scale;
        if let Some(codes) = self.qr_codes {
            let mut paint = Paint::default();
            paint.set_color_rgba8(20, 184, 166, 255);
            let stroke = Stroke {
                width: (1.8 * s).max(1.0),
                ..Default::default()
            };
            for code in codes {
                if let Some(rect) = SkiaRect::from_xywh(
                    code.bounds.x as f32,
                    code.bounds.y as f32,
                    code.bounds.width as f32,
                    code.bounds.height as f32,
                ) {
                    let mut pb = PathBuilder::new();
                    pb.push_rect(rect);
                    if let Some(path) = pb.finish() {
                        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                    }
                }
            }
        }
    }

    /// 马赛克或橡皮擦画笔模式在光标位置绘制实际大小的圆形笔刷轮廓。
    pub(super) fn render_active_brush_cursor(&self, pixmap: &mut PixmapMut, selection: &Rect) {
        let Some(toolbar) = self.toolbar else {
            return;
        };
        let cursor = (self.cursor_pos.0 as f32, self.cursor_pos.1 as f32);
        if !self.cursor_ready
            || !selection.contains(cursor.0, cursor.1)
            || toolbar.contains_point(cursor.0, cursor.1)
        {
            return;
        }
        let is_brush = match toolbar.active_tool {
            Some(Tool::Mosaic) => toolbar.sub_options.mosaic_brush_mode,
            Some(Tool::Eraser) => !toolbar.sub_options.eraser_rectangle_mode,
            _ => false,
        };
        if !is_brush {
            return;
        }
        let radius = (toolbar.sub_options.custom_stroke_width * 0.5 * self.ui_scale).max(2.0);
        let mut builder = PathBuilder::new();
        builder.push_circle(cursor.0, cursor.1, radius);
        let Some(path) = builder.finish() else {
            return;
        };
        let mut outline = Paint::default();
        outline.set_color_rgba8(15, 23, 42, 220);
        pixmap.stroke_path(
            &path,
            &outline,
            &Stroke {
                width: 3.0 * self.ui_scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
        let mut inner = Paint::default();
        inner.set_color_rgba8(255, 255, 255, 240);
        pixmap.stroke_path(
            &path,
            &inner,
            &Stroke {
                width: 1.4 * self.ui_scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
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
    /// 与 `render_status_message` 共用同一布局函数，供增量重绘精确抹除旧提示。
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

    /// 绘制 OCR 行左侧复选框及选中勾。
    fn render_ocr_checkbox(pixmap: &mut PixmapMut, row: &Rect, selected: bool, s: f32) {
        let box_size = 12.0 * s;
        let box_x = row.x + 7.0 * s;
        let box_y = row.y + (row.height - box_size) / 2.0;
        let mut paint = Paint::default();
        paint.set_color_rgba8(241, 245, 249, 255);
        let Some(rect) = SkiaRect::from_xywh(box_x, box_y, box_size, box_size) else {
            return;
        };
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
        if !selected {
            return;
        }
        let mut check = PathBuilder::new();
        check.move_to(box_x + 2.5 * s, box_y + 6.0 * s);
        check.line_to(box_x + 5.0 * s, box_y + 9.0 * s);
        check.line_to(box_x + 10.0 * s, box_y + 3.0 * s);
        let Some(path) = check.finish() else {
            return;
        };
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
