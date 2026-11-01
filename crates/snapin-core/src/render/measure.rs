//! 文本测量：输入框行宽与光标右边界偏移。

use super::text::TextRenderer;
use cosmic_text::{Attrs, Buffer, Color as CosmicColor, Metrics, Shaping};

impl TextRenderer {
    /// 测量输入框文本布局尺寸，返回行宽 (line_w) 与相对末尾字形右边界偏移量 (rel_cursor_x)
    pub fn measure_text_input(
        &mut self,
        text: &str,
        font_size: f32,
        line_height: f32,
        max_w: f32,
        max_h: f32,
    ) -> (f32, f32) {
        if text.is_empty() {
            return (0.0, 0.0);
        }

        let metrics = Metrics::new(font_size, line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, max_w, max_h);
        buffer.set_text(&mut self.font_system, text, Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut line_w: f32 = 0.0;
        for run in buffer.layout_runs() {
            if run.line_w > line_w {
                line_w = run.line_w;
            }
        }

        let mut max_gx: f32 = 0.0;
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            CosmicColor::rgba(255, 255, 255, 255),
            |gx, _gy, w, _h, _c| {
                if (gx + w as i32) as f32 > max_gx {
                    max_gx = (gx + w as i32) as f32;
                }
            },
        );

        let cursor_offset = max_gx + 2.0;
        (line_w, cursor_offset)
    }
}
