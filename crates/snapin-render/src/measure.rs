//! 文本测量：输入框最大行宽、多行总高度与末尾光标行列偏移。

use super::text::{TextMeasureOptions, TextRenderer};
use cosmic_text::{Attrs, Buffer, Metrics, Shaping};

/// 文本输入框布局与光标度量信息。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextInputMetrics {
    /// 所有行中的最大行宽
    pub max_line_width: f32,
    /// 多行总布局高度
    pub total_height: f32,
    /// 末行光标相对左上角 (0, 0) 的 X 偏移量
    pub cursor_x: f32,
    /// 末行光标相对左上角 (0, 0) 的 Y 偏移量
    pub cursor_y: f32,
}

impl TextRenderer {
    /// 测量输入框文本多行排版尺寸，返回最大行宽、总高度与末尾光标偏移坐标。
    pub fn measure_text_input(
        &mut self,
        text: &str,
        options: TextMeasureOptions,
    ) -> TextInputMetrics {
        if text.is_empty() {
            return TextInputMetrics {
                max_line_width: 0.0,
                total_height: options.line_height,
                cursor_x: 0.0,
                cursor_y: 0.0,
            };
        }

        let metrics = Metrics::new(options.font_size, options.line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, options.max_width, options.max_height);
        buffer.set_text(&mut self.font_system, text, Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut max_line_w: f32 = 0.0;
        let mut last_line_w: f32 = 0.0;
        let mut line_count: usize = 0;

        for run in buffer.layout_runs() {
            line_count = line_count.max(run.line_i + 1);
            if run.line_w > max_line_w {
                max_line_w = run.line_w;
            }
            last_line_w = run.line_w;
        }

        let lines = if text.ends_with('\n') {
            line_count.saturating_add(1)
        } else {
            line_count.max(1)
        };

        let cursor_y = (lines.saturating_sub(1) as f32) * options.line_height;
        let cursor_x = if text.ends_with('\n') {
            0.0
        } else {
            last_line_w + 2.0
        };

        TextInputMetrics {
            max_line_width: max_line_w,
            total_height: (lines as f32) * options.line_height,
            cursor_x,
            cursor_y,
        }
    }
}
