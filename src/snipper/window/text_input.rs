//! 文本标注实时输入会话管理与即时输入光标渲染。

use crate::annotation::action::AnnotationAction;
use crate::annotation::shapes::TextAction;
use cosmic_text::{Attrs, Buffer, Color as CosmicColor, FontSystem, Metrics, Shaping, SwashCache};
use std::cell::RefCell;
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

thread_local! {
    static FONT_SYSTEM: RefCell<(FontSystem, SwashCache)> = RefCell::new((
        FontSystem::new(),
        SwashCache::new(),
    ));
}

/// 正在进行的文本标注交互输入会话
#[derive(Debug, Clone)]
pub struct TextInputSession {
    /// 文本起始左上角坐标 (x, y)
    pub pos: (f32, f32),
    /// 已输入的文本字符内容
    pub text: String,
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

    /// 删除末尾字符
    pub fn pop_char(&mut self) {
        self.text.pop();
    }

    /// 动态调节字号
    pub fn adjust_font_size(&mut self, delta: f32) {
        self.font_size = (self.font_size + delta).clamp(12.0, 72.0);
    }

    /// 将当前会话转换为持久化标注动作
    pub fn commit(self) -> Option<Box<dyn AnnotationAction>> {
        let trimmed = self.text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Box::new(TextAction {
                pos: self.pos,
                text: self.text,
                color: self.color,
                font_size: self.font_size,
            }))
        }
    }

    /// 在画布上渲染当前输入的文字以及尾随光标
    pub fn render(&self, pixmap: &mut PixmapMut) {
        let (x, y) = self.pos;
        let font_size = self.font_size.max(8.0);
        let line_height = font_size * 1.25;

        let mut cursor_x = x;
        let cursor_y = y;

        if !self.text.is_empty() {
            cursor_x = self.draw_text_content(pixmap, x, y, font_size, line_height);
        }

        render_input_cursor(pixmap, cursor_x, cursor_y, font_size, self.color);
    }

    /// 绘制当前已输入的文字内容并返回末尾光标 X 坐标
    fn draw_text_content(
        &self,
        pixmap: &mut PixmapMut,
        x: f32,
        y: f32,
        font_size: f32,
        line_height: f32,
    ) -> f32 {
        let mut cursor_x = x;

        FONT_SYSTEM.with(|cell| {
            let mut borrowed = cell.borrow_mut();
            let (font_system, swash_cache) = &mut *borrowed;

            let metrics = Metrics::new(font_size, line_height);
            let mut buffer = Buffer::new(font_system, metrics);
            buffer.set_size(font_system, pixmap.width() as f32, pixmap.height() as f32);
            buffer.set_text(font_system, &self.text, Attrs::new(), Shaping::Advanced);
            buffer.shape_until_scroll(font_system, false);

            let c_color = CosmicColor::rgba(self.color.0, self.color.1, self.color.2, self.color.3);
            let width = pixmap.width() as i32;
            let height = pixmap.height() as i32;

            let mut max_gx: f32 = x;

            buffer.draw(font_system, swash_cache, c_color, |gx, gy, w, _h, c| {
                let px = x as i32 + gx;
                let py = y as i32 + gy;
                if (gx + w as i32) as f32 + x > max_gx {
                    max_gx = (gx + w as i32) as f32 + x;
                }
                if px >= 0 && px < width && py >= 0 && py < height {
                    let idx = ((py as usize) * (width as usize) + (px as usize)) * 4;
                    let data = pixmap.data_mut();
                    let src_a = c.a() as f32 / 255.0;
                    let dst_a = data[idx + 3] as f32 / 255.0;
                    let out_a = src_a + dst_a * (1.0 - src_a);
                    if out_a > 0.0 {
                        let src_r = c.r() as f32 * src_a;
                        let src_g = c.g() as f32 * src_a;
                        let src_b = c.b() as f32 * src_a;
                        let dst_r = data[idx] as f32 * dst_a;
                        let dst_g = data[idx + 1] as f32 * dst_a;
                        let dst_b = data[idx + 2] as f32 * dst_a;
                        data[idx] = ((src_r + dst_r * (1.0 - src_a)) / out_a) as u8;
                        data[idx + 1] = ((src_g + dst_g * (1.0 - src_a)) / out_a) as u8;
                        data[idx + 2] = ((src_b + dst_b * (1.0 - src_a)) / out_a) as u8;
                        data[idx + 3] = (out_a * 255.0) as u8;
                    }
                }
            });

            cursor_x = max_gx + 2.0;
        });

        cursor_x
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
}
