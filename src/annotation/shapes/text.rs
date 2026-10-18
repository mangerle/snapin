//! 文本标注图元绘制实现。

use crate::annotation::action::AnnotationAction;
use cosmic_text::{Attrs, Buffer, Color as CosmicColor, FontSystem, Metrics, Shaping, SwashCache};
use std::cell::RefCell;
use tiny_skia::PixmapMut;

thread_local! {
    static FONT_SYSTEM: RefCell<(FontSystem, SwashCache)> = RefCell::new((
        FontSystem::new(),
        SwashCache::new(),
    ));
}

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

        FONT_SYSTEM.with(|cell| {
            let mut borrowed = cell.borrow_mut();
            let (font_system, swash_cache) = &mut *borrowed;

            let metrics = Metrics::new(font_size, font_size * 1.25);
            let mut buffer = Buffer::new(font_system, metrics);
            buffer.set_size(font_system, pixmap.width() as f32, pixmap.height() as f32);
            buffer.set_text(font_system, &self.text, Attrs::new(), Shaping::Advanced);
            buffer.shape_until_scroll(font_system, false);

            let c_color = CosmicColor::rgba(self.color.0, self.color.1, self.color.2, self.color.3);
            let width = pixmap.width() as i32;
            let height = pixmap.height() as i32;

            buffer.draw(font_system, swash_cache, c_color, |gx, gy, _w, _h, c| {
                let px = x as i32 + gx;
                let py = y as i32 + gy;
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
        });
    }

    fn adjust_size(&mut self, _stroke_w: f32, font_sz: f32) {
        self.font_size = font_sz;
    }
}
