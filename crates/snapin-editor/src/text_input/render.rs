//! 文本输入会话软光栅化渲染：输入框衬底、选区高亮、即时文字与光标。

use super::session::TextInputSession;
use snapin_render::{
    with_thread_local_text_renderer, SelectionRect, TextDrawOptions, TextMeasureOptions,
};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

impl TextInputSession {
    /// 在画布上渲染当前输入的文字以及光标与黑色边界框
    pub fn render(&self, pixmap: &mut PixmapMut) {
        self.render_scaled(pixmap, 1.0, false);
    }

    /// 按给定缩放倍率渲染输入框、选区高亮、文字与光标
    pub fn render_scaled(&self, pixmap: &mut PixmapMut, scale: f32, underline_preedit: bool) {
        let scale = if scale > f32::EPSILON { scale } else { 1.0 };
        let (x, y) = (self.pos.x * scale, self.pos.y * scale);
        let font_size = (self.font_size * scale).max(8.0 * scale.min(1.0));
        let line_height = font_size * 1.25;

        // 构造排版文本：若有 IME 预编辑文本，则插入在当前光标处供实时预览
        let (display_text, display_cursor) = if self.preedit.is_empty() {
            (std::borrow::Cow::Borrowed(&self.text), self.cursor)
        } else {
            let mut s = self.text[..self.cursor].to_string();
            s.push_str(&self.preedit);
            s.push_str(&self.text[self.cursor..]);
            let new_cur = self.cursor + self.preedit.len();
            (std::borrow::Cow::Owned(s), new_cur)
        };

        let measure_options = TextMeasureOptions {
            font_size,
            line_height,
            max_width: pixmap.width() as f32,
            max_height: pixmap.height() as f32,
        };

        if display_text.is_empty() {
            let min_w = (font_size * 3.0).max(64.0 * scale);
            render_input_box(
                pixmap,
                InputBoxRect {
                    x: x - 6.0 * scale,
                    y: y - 4.0 * scale,
                    width: min_w,
                    height: line_height + 8.0 * scale,
                },
            );
            render_input_cursor(
                pixmap,
                TextCursorSpec {
                    x,
                    y,
                    font_size,
                    color: self.color,
                },
            );
            return;
        }

        let frame = RenderFrame {
            x,
            y,
            font_size,
            scale,
            text: display_text.as_ref(),
            display_cursor,
            underline_preedit,
        };

        self.draw_box_and_content(pixmap, &frame, measure_options);
    }

    /// 测量排版、绘制背景框、选区高亮、文字与光标
    fn draw_box_and_content(
        &self,
        pixmap: &mut PixmapMut,
        frame: &RenderFrame<'_>,
        options: TextMeasureOptions,
    ) {
        with_thread_local_text_renderer(|renderer| {
            let metrics = renderer.measure_text_cursor(frame.text, frame.display_cursor, options);
            draw_input_background(pixmap, frame, metrics.max_line_width, metrics.total_height);

            // 若存在文本选区，绘制淡蓝色半透明选区高亮
            if let Some((sel_start, sel_end)) = self.selection_range() {
                let rects =
                    renderer.measure_selection_rects(frame.text, sel_start, sel_end, options);
                render_selection_rects(pixmap, frame.x, frame.y, &rects);
            }

            // 绘制文字正文
            renderer.draw_text_rgba(
                pixmap,
                frame.text,
                TextDrawOptions::new(frame.x, frame.y, frame.font_size, self.color),
            );

            // 绘制 IME 预编辑下划线
            if frame.underline_preedit && !self.preedit.is_empty() {
                let preedit_rects = renderer.measure_selection_rects(
                    frame.text,
                    self.cursor,
                    self.cursor + self.preedit.len(),
                    options,
                );
                for r in preedit_rects {
                    render_preedit_underline(
                        pixmap,
                        PreeditUnderline {
                            start_x: frame.x + r.x,
                            end_x: frame.x + r.x + r.width,
                            y: frame.y + r.y + frame.font_size * 1.15,
                            scale: frame.scale,
                        },
                    );
                }
            }

            // 绘制输入光标竖线
            render_input_cursor(
                pixmap,
                TextCursorSpec {
                    x: frame.x + metrics.cursor_x,
                    y: frame.y + metrics.cursor_y,
                    font_size: frame.font_size,
                    color: self.color,
                },
            );
        });
    }
}

/// 渲染参数快照
struct RenderFrame<'a> {
    x: f32,
    y: f32,
    font_size: f32,
    scale: f32,
    text: &'a str,
    display_cursor: usize,
    underline_preedit: bool,
}

/// 输入框外接矩形
struct InputBoxRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// 输入光标描述
struct TextCursorSpec {
    x: f32,
    y: f32,
    font_size: f32,
    color: (u8, u8, u8, u8),
}

/// 预编辑下划线描述
struct PreeditUnderline {
    start_x: f32,
    end_x: f32,
    y: f32,
    scale: f32,
}

/// 绘制输入框外接圆角背景
fn draw_input_background(
    pixmap: &mut PixmapMut,
    frame: &RenderFrame<'_>,
    max_line_width: f32,
    total_height: f32,
) {
    let min_width = (frame.font_size * 3.0).max(64.0 * frame.scale);
    let box_width = (max_line_width + 16.0 * frame.scale).max(min_width);
    let box_height = total_height + 8.0 * frame.scale;
    render_input_box(
        pixmap,
        InputBoxRect {
            x: frame.x - 6.0 * frame.scale,
            y: frame.y - 4.0 * frame.scale,
            width: box_width,
            height: box_height,
        },
    );
}

/// 绘制文字输入过程中的深色外边框与半透明浅白背景衬底
fn render_input_box(pixmap: &mut PixmapMut, input_box: InputBoxRect) {
    let Some(path) = input_box_path(input_box) else {
        return;
    };

    let mut background = Paint {
        anti_alias: true,
        ..Default::default()
    };
    background.set_color_rgba8(255, 255, 255, 210);
    pixmap.fill_path(
        &path,
        &background,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );

    let mut border = Paint {
        anti_alias: true,
        ..Default::default()
    };
    border.set_color_rgba8(15, 23, 42, 240);
    pixmap.stroke_path(
        &path,
        &border,
        &Stroke {
            width: 1.5,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
}

/// 构造输入框圆角路径
fn input_box_path(input_box: InputBoxRect) -> Option<tiny_skia::Path> {
    let InputBoxRect {
        x,
        y,
        width,
        height,
    } = input_box;
    let radius = 2.5;
    let mut builder = PathBuilder::new();
    builder.move_to(x + radius, y);
    builder.line_to(x + width - radius, y);
    builder.quad_to(x + width, y, x + width, y + radius);
    builder.line_to(x + width, y + height - radius);
    builder.quad_to(x + width, y + height, x + width - radius, y + height);
    builder.line_to(x + radius, y + height);
    builder.quad_to(x, y + height, x, y + height - radius);
    builder.line_to(x, y + radius);
    builder.quad_to(x, y, x + radius, y);
    builder.close();
    builder.finish()
}

/// 绘制选区高亮半透明蓝色矩形
fn render_selection_rects(
    pixmap: &mut PixmapMut,
    origin_x: f32,
    origin_y: f32,
    rects: &[SelectionRect],
) {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    // 现代半透明高亮蓝 (59, 130, 246, 110)
    paint.set_color_rgba8(59, 130, 246, 110);

    for r in rects {
        let rect_x = origin_x + r.x;
        let rect_y = origin_y + r.y;
        if let Some(rect) = tiny_skia::Rect::from_xywh(rect_x, rect_y, r.width.max(2.0), r.height) {
            pixmap.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }
}

/// 绘制预编辑文本下划线
fn render_preedit_underline(pixmap: &mut PixmapMut, underline: PreeditUnderline) {
    let PreeditUnderline {
        start_x: x0,
        end_x: x1,
        y,
        scale,
    } = underline;
    if x1 <= x0 {
        return;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(x0, y);
    pb.line_to(x1, y);
    if let Some(path) = pb.finish() {
        let mut paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        paint.set_color_rgba8(15, 23, 42, 230);
        let stroke = Stroke {
            width: 1.5 * scale.max(0.1),
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制输入光标指示竖线
fn render_input_cursor(pixmap: &mut PixmapMut, cursor: TextCursorSpec) {
    let TextCursorSpec {
        x: cx,
        y: cy,
        font_size,
        color,
    } = cursor;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy);
    pb.line_to(cx, cy + font_size * 1.15);
    if let Some(path) = pb.finish() {
        let mut paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        paint.set_color_rgba8(color.0, color.1, color.2, 255);
        let stroke = Stroke {
            width: 2.0,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}
