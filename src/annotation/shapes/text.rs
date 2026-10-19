//! 文本标注图元绘制实现。

use crate::annotation::action::AnnotationAction;
use crate::render::with_thread_local_text_renderer;
use tiny_skia::PixmapMut;

/// 文本标注图元动作
#[derive(Debug, Clone)]
pub struct TextAction {
    pub pos: (f32, f32),
    pub text: String,
    pub color: (u8, u8, u8, u8),
    pub font_size: f32,
}

impl AnnotationAction for TextAction {
    fn draw(&self, pixmap: &mut PixmapMut) {
        if self.text.is_empty() {
            return;
        }

        let (x, y) = self.pos;
        let font_size = self.font_size.max(8.0);

        with_thread_local_text_renderer(|renderer| {
            renderer.draw_text_rgba(pixmap, &self.text, x, y, font_size, self.color);
        });
    }

    fn adjust_size(&mut self, _stroke_w: f32, font_sz: f32) {
        self.font_size = font_sz;
    }
}
