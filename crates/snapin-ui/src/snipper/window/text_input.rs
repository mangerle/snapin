//! 文本标注实时输入会话管理与即时输入光标渲染。

use snapin_core::annotation::action::AnnotationShape;
use snapin_core::annotation::shapes::TextAction;
use snapin_core::render::with_thread_local_text_renderer;
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 正在进行的文本标注交互输入会话
#[derive(Debug, Clone)]
pub struct TextInputSession {
    /// 文本起始左上角坐标 (x, y)
    pub pos: (f32, f32),
    /// 已提交确认的文本字符内容
    pub text: String,
    /// 候选输入法预编辑拼音内容 (IME Preedit)
    pub preedit: String,
    /// 文字颜色 (r, g, b, a)
    pub color: (u8, u8, u8, u8),
    /// 字体字号大小 (受鼠标滚轮连续动态微调，范围 12.0 ~ 72.0)
    pub font_size: f32,
}

impl TextInputSession {
    /// 创建新的文本输入会话
    pub fn new(pos: (f32, f32), color: (u8, u8, u8, u8), font_size: f32) -> Self {
        Self {
            pos,
            text: String::with_capacity(64),
            preedit: String::new(),
            color,
            font_size: font_size.clamp(12.0, 72.0),
        }
    }

    /// 追加单个字符
    pub fn push_char(&mut self, ch: char) {
        self.text.push(ch);
    }

    /// 追加字符串切片
    pub fn push_str(&mut self, s: &str) {
        self.text.push_str(s);
    }

    /// 接收输入法确认提交的中文/外文文本
    pub fn push_commit(&mut self, s: &str) {
        self.preedit.clear();
        self.text.push_str(s);
    }

    /// 接收输入法候选状态变动
    pub fn set_preedit(&mut self, s: &str) {
        self.preedit = s.to_string();
    }

    /// 删除末尾字符
    pub fn pop_char(&mut self) {
        if !self.preedit.is_empty() {
            self.preedit.pop();
        } else {
            self.text.pop();
        }
    }

    /// 动态调节字号
    pub fn adjust_font_size(&mut self, delta: f32) {
        self.font_size = (self.font_size + delta).clamp(12.0, 72.0);
    }

    /// 将当前会话转换为持久化标注动作
    pub fn commit(mut self) -> Option<AnnotationShape> {
        if !self.preedit.is_empty() {
            self.text.push_str(&self.preedit);
            self.preedit.clear();
        }
        let trimmed = self.text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(AnnotationShape::Text(TextAction {
                pos: self.pos,
                text: self.text,
                color: self.color,
                font_size: self.font_size,
            }))
        }
    }

    /// 在画布上渲染当前输入的文字以及尾随光标与黑色边界框
    pub fn render(&self, pixmap: &mut PixmapMut) {
        self.render_scaled(pixmap, 1.0, false);
    }

    /// 按给定缩放倍率渲染输入框、文字与光标，可选为预编辑文本绘制下划线。
    ///
    /// 截图窗口的会话坐标本身就是屏幕物理像素，因此 [`Self::render`] 以 1.0 倍率调用；
    /// 贴图窗口的会话坐标是图像像素，渲染时必须乘以图像缩放倍率才能与屏幕对齐，
    /// 字号、内边距与光标也按同一倍率放大，保证预览与最终烘焙结果一致。
    /// `underline_preedit` 仅供贴图预览启用，避免改变截图侧既有的输入视觉效果。
    pub(crate) fn render_scaled(
        &self,
        pixmap: &mut PixmapMut,
        scale: f32,
        underline_preedit: bool,
    ) {
        let scale = if scale > f32::EPSILON { scale } else { 1.0 };
        let (x, y) = (self.pos.0 * scale, self.pos.1 * scale);
        // 缩小贴图时字号必须同步缩小；仅放大或 1:1 时保留 8 像素可读性下限
        let font_size = (self.font_size * scale).max(8.0 * scale.min(1.0));
        let line_height = font_size * 1.25;

        let display_text = if self.preedit.is_empty() {
            std::borrow::Cow::Borrowed(&self.text)
        } else {
            std::borrow::Cow::Owned(format!("{}{}", self.text, self.preedit))
        };

        let cursor_x = if display_text.is_empty() {
            let min_w = (font_size * 3.0).max(64.0 * scale);
            render_input_box(
                pixmap,
                x - 6.0 * scale,
                y - 4.0 * scale,
                min_w,
                line_height + 8.0 * scale,
            );
            x
        } else {
            let frame = InputFrame {
                x,
                y,
                font_size,
                line_height,
                scale,
                text: display_text.as_ref(),
                underline_preedit,
            };
            self.draw_box_and_text(pixmap, &frame)
        };

        render_input_cursor(pixmap, cursor_x, y, font_size, self.color);
    }

    /// 测量输入框范围、绘制黑色外框底衬与文字像素，并返回末尾光标 X 坐标
    fn draw_box_and_text(&self, pixmap: &mut PixmapMut, frame: &InputFrame<'_>) -> f32 {
        with_thread_local_text_renderer(|renderer| {
            let max_w = pixmap.width() as f32;
            let max_h = pixmap.height() as f32;
            let (line_w, cursor_offset) = renderer.measure_text_input(
                frame.text,
                frame.font_size,
                frame.line_height,
                max_w,
                max_h,
            );

            // 1. 优先绘制黑色外框与底衬
            let min_w = (frame.font_size * 3.0).max(64.0 * frame.scale);
            let box_w = (line_w + 16.0 * frame.scale).max(min_w);
            let box_h = frame.line_height + 8.0 * frame.scale;
            render_input_box(
                pixmap,
                frame.x - 6.0 * frame.scale,
                frame.y - 4.0 * frame.scale,
                box_w,
                box_h,
            );

            // 2. 绘制文字像素
            renderer.draw_text_rgba(
                pixmap,
                frame.text,
                frame.x,
                frame.y,
                frame.font_size,
                self.color,
            );

            // 3. 预编辑文本以深色下划线标示，与已确认文本区分
            if frame.underline_preedit && !self.preedit.is_empty() {
                let (_, text_cursor) = renderer.measure_text_input(
                    &self.text,
                    frame.font_size,
                    frame.line_height,
                    max_w,
                    max_h,
                );
                let start_x = if self.text.is_empty() {
                    frame.x
                } else {
                    frame.x + text_cursor
                };
                let end_x = frame.x + cursor_offset;
                render_preedit_underline(
                    pixmap,
                    start_x,
                    end_x,
                    frame.y + frame.font_size * 1.15,
                    frame.scale,
                );
            }

            frame.x + cursor_offset
        })
    }
}

/// 单帧输入框渲染参数，避免缩放渲染路径平铺过多入参。
struct InputFrame<'a> {
    x: f32,
    y: f32,
    font_size: f32,
    line_height: f32,
    scale: f32,
    text: &'a str,
    underline_preedit: bool,
}

/// 绘制文字输入过程中的黑色外边框与高对比度浅白背景衬底
fn render_input_box(pixmap: &mut PixmapMut, box_x: f32, box_y: f32, box_w: f32, box_h: f32) {
    let mut pb = PathBuilder::new();
    let r = 2.5;
    pb.move_to(box_x + r, box_y);
    pb.line_to(box_x + box_w - r, box_y);
    pb.quad_to(box_x + box_w, box_y, box_x + box_w, box_y + r);
    pb.line_to(box_x + box_w, box_y + box_h - r);
    pb.quad_to(
        box_x + box_w,
        box_y + box_h,
        box_x + box_w - r,
        box_y + box_h,
    );
    pb.line_to(box_x + r, box_y + box_h);
    pb.quad_to(box_x, box_y + box_h, box_x, box_y + box_h - r);
    pb.line_to(box_x, box_y + r);
    pb.quad_to(box_x, box_y, box_x + r, box_y);
    pb.close();

    if let Some(path) = pb.finish() {
        // 1. 半透明微白底色，确保在深色或复杂底图上文字和光标依然清晰
        let mut bg_paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        bg_paint.set_color_rgba8(255, 255, 255, 210);
        pixmap.fill_path(
            &path,
            &bg_paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        // 2. 经典深黑外边框
        let mut border_paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        border_paint.set_color_rgba8(15, 23, 42, 240);
        let stroke = Stroke {
            width: 1.5,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制预编辑文本下划线，示意该段文字仍处于输入法候选状态。
fn render_preedit_underline(pixmap: &mut PixmapMut, x0: f32, x1: f32, y: f32, scale: f32) {
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
fn render_input_cursor(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    font_size: f32,
    color: (u8, u8, u8, u8),
) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    #[test]
    fn test_text_input_session_lifecycle() {
        let mut session = TextInputSession::new((50.0, 50.0), (255, 0, 0, 255), 18.0);
        assert_eq!(session.text, "");

        session.push_char('H');
        session.push_str("ello 世界");
        assert_eq!(session.text, "Hello 世界");

        session.pop_char();
        assert_eq!(session.text, "Hello 世");

        // 验证 IME 预编辑与提交
        session.set_preedit("jie");
        assert_eq!(session.preedit, "jie");
        session.push_commit("界！");
        assert_eq!(session.text, "Hello 世界！");
        assert_eq!(session.preedit, "");

        session.adjust_font_size(4.0);
        assert_eq!(session.font_size, 22.0);

        let action = session.commit();
        assert!(action.is_some());
    }

    #[test]
    fn test_empty_text_input_commit_none() {
        let session = TextInputSession::new((50.0, 50.0), (255, 0, 0, 255), 18.0);
        assert!(session.commit().is_none());
    }

    #[test]
    fn test_text_input_render_pixels() {
        let mut pixmap = Pixmap::new(200, 100).unwrap();
        pixmap.fill(tiny_skia::Color::BLACK);

        let mut session = TextInputSession::new((10.0, 20.0), (255, 255, 255, 255), 20.0);
        session.push_str("测试文本");
        session.render(&mut pixmap.as_mut());

        // 验证绘制了非黑色像素 (包含文字与光标)
        let has_content = pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content);
    }

    #[test]
    fn test_empty_text_input_renders_box() {
        let mut pixmap = Pixmap::new(200, 100).unwrap();
        pixmap.fill(tiny_skia::Color::BLACK);

        let session = TextInputSession::new((10.0, 20.0), (255, 0, 0, 255), 20.0);
        assert!(session.text.is_empty());
        session.render(&mut pixmap.as_mut());

        // 验证空输入时仍绘制了黑色外框底衬与光标
        let has_pixels = pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_pixels);
    }
}
