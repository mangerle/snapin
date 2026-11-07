//! 文本测量：输入框最大行宽、多行总高度、光标坐标定位、命中测试与选区度量。

use super::text::{TextMeasureOptions, TextRenderer};
use cosmic_text::{Attrs, Buffer, Metrics, Shaping};

/// 文本输入框布局与光标度量信息。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextInputMetrics {
    /// 所有行中的最大行宽
    pub max_line_width: f32,
    /// 多行总布局高度
    pub total_height: f32,
    /// 指定光标处相对左上角 (0, 0) 的 X 偏移量
    pub cursor_x: f32,
    /// 指定光标处相对左上角 (0, 0) 的 Y 偏移量
    pub cursor_y: f32,
}

/// 选区单行高亮矩形 (x, y, width, height)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// 计算各物理行在全文中的绝对起始字节索引
fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::with_capacity(text.lines().count() + 1);
    starts.push(0);
    for (idx, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push(idx + 1);
        }
    }
    starts
}

/// 将字节偏移夹取到最邻近的有效 UTF-8 字符边界
fn clamp_to_char_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

impl TextRenderer {
    /// 测量输入框文本多行排版尺寸，返回最大行宽、总高度与末尾光标偏移坐标。
    pub fn measure_text_input(
        &mut self,
        text: &str,
        options: TextMeasureOptions,
    ) -> TextInputMetrics {
        self.measure_text_cursor(text, text.len(), options)
    }

    /// 测量输入框文本并在指定字节偏移处计算光标位置。
    pub fn measure_text_cursor(
        &mut self,
        text: &str,
        cursor_offset: usize,
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

        let buffer = self.create_shaped_buffer(text, options);
        let starts = compute_line_starts(text);
        let safe_offset = clamp_to_char_boundary(text, cursor_offset);

        let mut max_line_w: f32 = 0.0;
        let mut line_count: usize = 0;
        for run in buffer.layout_runs() {
            line_count = line_count.max(run.line_i + 1);
            if run.line_w > max_line_w {
                max_line_w = run.line_w;
            }
        }

        let total_lines = if text.ends_with('\n') {
            line_count.saturating_add(1)
        } else {
            line_count.max(1)
        };
        let total_height = (total_lines as f32) * options.line_height;

        let (cursor_x, cursor_y) =
            self.locate_cursor(&buffer, text, &starts, safe_offset, options.line_height);

        TextInputMetrics {
            max_line_width: max_line_w,
            total_height,
            cursor_x,
            cursor_y,
        }
    }

    /// 点击命中测试：将相对于文本左上角的 (hit_x, hit_y) 转换为最近的 UTF-8 字符字节偏移。
    pub fn hit_test_text_offset(
        &mut self,
        text: &str,
        hit_x: f32,
        hit_y: f32,
        options: TextMeasureOptions,
    ) -> usize {
        if text.is_empty() {
            return 0;
        }

        let buffer = self.create_shaped_buffer(text, options);
        let starts = compute_line_starts(text);
        let runs: Vec<_> = buffer.layout_runs().collect();
        if runs.is_empty() {
            return 0;
        }

        // 若点击在尾随空行区域，直接返回末尾
        if text.ends_with('\n') {
            let last_bottom = runs
                .last()
                .map(|r| r.line_top + options.line_height)
                .unwrap_or(0.0);
            if hit_y >= last_bottom {
                return text.len();
            }
        }

        let selected_run = find_run_by_y(&runs, hit_y, options.line_height);
        let offset = hit_test_in_run(selected_run, &starts, hit_x);
        clamp_to_char_boundary(text, offset)
    }

    /// 计算给定选区范围在每一行的高亮矩形列表。
    pub fn measure_selection_rects(
        &mut self,
        text: &str,
        sel_start: usize,
        sel_end: usize,
        options: TextMeasureOptions,
    ) -> Vec<SelectionRect> {
        let (start, end) = (sel_start.min(sel_end), sel_start.max(sel_end));
        if text.is_empty() || start == end {
            return Vec::new();
        }

        let buffer = self.create_shaped_buffer(text, options);
        let starts = compute_line_starts(text);
        let mut rects = Vec::new();

        for run in buffer.layout_runs() {
            let base = starts.get(run.line_i).copied().unwrap_or(0);
            if run.glyphs.is_empty() {
                if start <= base && end > base {
                    rects.push(SelectionRect {
                        x: 0.0,
                        y: run.line_top,
                        width: options.font_size * 0.5,
                        height: options.line_height,
                    });
                }
                continue;
            }

            let first_glyph = &run.glyphs[0];
            let last_glyph = &run.glyphs[run.glyphs.len() - 1];
            let run_start = base + first_glyph.start;
            let run_end = base + last_glyph.end;

            if end <= run_start || start >= run_end {
                continue;
            }

            let mut min_x = f32::MAX;
            let mut max_x = 0.0f32;
            for g in run.glyphs.iter() {
                let g_start = base + g.start;
                let g_end = base + g.end;
                if g_end > start && g_start < end {
                    min_x = min_x.min(g.x);
                    max_x = max_x.max(g.x + g.w);
                }
            }

            if min_x <= max_x {
                rects.push(SelectionRect {
                    x: min_x,
                    y: run.line_top,
                    width: max_x - min_x,
                    height: options.line_height,
                });
            }
        }

        rects
    }

    /// 创建并完成文字排版的 Cosmic Buffer
    fn create_shaped_buffer(&mut self, text: &str, options: TextMeasureOptions) -> Buffer {
        let metrics = Metrics::new(options.font_size, options.line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, options.max_width, options.max_height);
        buffer.set_text(&mut self.font_system, text, Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }

    /// 在排版结果中精准定位指定字节偏移的光标坐标
    fn locate_cursor(
        &self,
        buffer: &Buffer,
        text: &str,
        starts: &[usize],
        offset: usize,
        line_height: f32,
    ) -> (f32, f32) {
        if offset == text.len() && text.ends_with('\n') {
            let last_top = buffer
                .layout_runs()
                .last()
                .map(|r| r.line_top)
                .unwrap_or(0.0);
            return (0.0, last_top + line_height);
        }

        let target_line = starts.iter().rposition(|&s| s <= offset).unwrap_or(0);
        let inline_offset = offset.saturating_sub(starts[target_line]);

        for run in buffer.layout_runs() {
            if run.line_i != target_line {
                continue;
            }
            if run.glyphs.is_empty() {
                return (0.0, run.line_top);
            }
            let first_start = run.glyphs[0].start;
            let last_end = run.glyphs[run.glyphs.len() - 1].end;

            if inline_offset < first_start {
                return (run.glyphs[0].x, run.line_top);
            }
            if inline_offset >= last_end {
                let last = &run.glyphs[run.glyphs.len() - 1];
                return (last.x + last.w, run.line_top);
            }

            for g in run.glyphs.iter() {
                if inline_offset <= g.start {
                    return (g.x, run.line_top);
                }
                if inline_offset < g.end {
                    return (g.x, run.line_top);
                }
            }
        }

        // 默认兜底
        (0.0, 0.0)
    }
}

/// 根据 Y 坐标匹配目标行 Run
fn find_run_by_y<'a, 'b>(
    runs: &'b [cosmic_text::LayoutRun<'a>],
    hit_y: f32,
    line_height: f32,
) -> &'b cosmic_text::LayoutRun<'a> {
    if hit_y <= 0.0 {
        return &runs[0];
    }
    for run in runs {
        if hit_y < run.line_top + line_height {
            return run;
        }
    }
    &runs[runs.len() - 1]
}

/// 在指定 Run 内根据 X 坐标命中最近字符偏移
fn hit_test_in_run(run: &cosmic_text::LayoutRun<'_>, starts: &[usize], hit_x: f32) -> usize {
    let base = starts.get(run.line_i).copied().unwrap_or(0);
    if run.glyphs.is_empty() {
        return base;
    }

    let first = &run.glyphs[0];
    if hit_x <= first.x {
        return base + first.start;
    }

    let last = &run.glyphs[run.glyphs.len() - 1];
    if hit_x >= last.x + last.w {
        return base + last.end;
    }

    for g in run.glyphs.iter() {
        if hit_x < g.x + g.w {
            let mid = g.x + g.w * 0.5;
            return if hit_x < mid {
                base + g.start
            } else {
                base + g.end
            };
        }
    }

    base + last.end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text_cursor() {
        let mut renderer = TextRenderer::new();
        let options = TextMeasureOptions {
            font_size: 16.0,
            line_height: 20.0,
            max_width: 500.0,
            max_height: 500.0,
        };
        let metrics = renderer.measure_text_cursor("", 0, options);
        assert_eq!(metrics.cursor_x, 0.0);
        assert_eq!(metrics.cursor_y, 0.0);
        assert_eq!(renderer.hit_test_text_offset("", 10.0, 10.0, options), 0);
    }

    #[test]
    fn test_single_line_cursor_and_hit_test() {
        let mut renderer = TextRenderer::new();
        let options = TextMeasureOptions {
            font_size: 16.0,
            line_height: 20.0,
            max_width: 500.0,
            max_height: 500.0,
        };
        let text = "Hello";
        let at_start = renderer.measure_text_cursor(text, 0, options);
        assert_eq!(at_start.cursor_x, 0.0);
        assert_eq!(at_start.cursor_y, 0.0);

        let at_end = renderer.measure_text_cursor(text, text.len(), options);
        assert!(at_end.cursor_x > 0.0);
        assert_eq!(at_end.cursor_y, 0.0);

        // 命中测试在开头左侧
        assert_eq!(renderer.hit_test_text_offset(text, -5.0, 10.0, options), 0);
        // 命中测试在末尾右侧
        assert_eq!(
            renderer.hit_test_text_offset(text, at_end.cursor_x + 50.0, 10.0, options),
            text.len()
        );
    }

    #[test]
    fn test_multiline_cursor_and_trailing_newline() {
        let mut renderer = TextRenderer::new();
        let options = TextMeasureOptions {
            font_size: 16.0,
            line_height: 20.0,
            max_width: 500.0,
            max_height: 500.0,
        };
        let text = "Line1\nLine2\n";
        // 光标在第二行开头
        let line2_start = "Line1\n".len();
        let at_line2 = renderer.measure_text_cursor(text, line2_start, options);
        assert_eq!(at_line2.cursor_x, 0.0);
        assert_eq!(at_line2.cursor_y, 20.0);

        // 光标在尾随换行符后的空行
        let at_trailing = renderer.measure_text_cursor(text, text.len(), options);
        assert_eq!(at_trailing.cursor_x, 0.0);
        assert_eq!(at_trailing.cursor_y, 40.0);

        // 命中测试点击第三行
        let hit_trailing = renderer.hit_test_text_offset(text, 10.0, 45.0, options);
        assert_eq!(hit_trailing, text.len());
    }

    #[test]
    fn test_selection_rects() {
        let mut renderer = TextRenderer::new();
        let options = TextMeasureOptions {
            font_size: 16.0,
            line_height: 20.0,
            max_width: 500.0,
            max_height: 500.0,
        };
        let text = "Hello World";
        let rects = renderer.measure_selection_rects(text, 0, 5, options);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].x, 0.0);
        assert!(rects[0].width > 0.0);
        assert_eq!(rects[0].height, 20.0);
    }
}
