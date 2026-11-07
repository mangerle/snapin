//! 文字识别全文编辑框核心状态与文本操作逻辑。

use super::layout::OcrEditorButton;
use std::borrow::Cow;
use std::time::Instant;

/// 文字识别全文编辑框内部状态。
#[derive(Debug, Clone)]
pub(crate) struct OcrTextEditorState {
    pub(crate) text: String,
    pub(crate) preedit: String,
    pub(crate) cursor: usize,
    pub(crate) selection_anchor: Option<usize>,
    pub(crate) scroll_y: f32,
    pub(crate) content_height: f32,
    pub(crate) custom_position: Option<(f32, f32)>,
    pub(crate) drag_start: Option<(f32, f32)>,
    pub(crate) is_selecting_text: bool,
    pub(crate) is_dragging_scrollbar: bool,
    pub(crate) scrollbar_drag_start_y: f32,
    pub(crate) scrollbar_drag_start_scroll_y: f32,
    pub(crate) hovered_button: Option<OcrEditorButton>,
    pub(crate) pressed_button: Option<OcrEditorButton>,
    pub(crate) blink_start: Instant,
}

impl PartialEq for OcrTextEditorState {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
            && self.preedit == other.preedit
            && self.cursor == other.cursor
            && self.selection_anchor == other.selection_anchor
            && (self.scroll_y - other.scroll_y).abs() < f32::EPSILON
    }
}

impl Eq for OcrTextEditorState {}

impl OcrTextEditorState {
    /// 使用 OCR 识别全文初始化编辑状态，光标默认置于末尾。
    pub(crate) fn new(text: String) -> Self {
        let cursor = text.len();
        Self {
            text,
            preedit: String::new(),
            cursor,
            selection_anchor: None,
            scroll_y: 0.0,
            content_height: 0.0,
            custom_position: None,
            drag_start: None,
            is_selecting_text: false,
            is_dragging_scrollbar: false,
            scrollbar_drag_start_y: 0.0,
            scrollbar_drag_start_scroll_y: 0.0,
            hovered_button: None,
            pressed_button: None,
            blink_start: Instant::now(),
        }
    }

    /// 当前完整正文引用（不含未提交预编辑）。
    #[cfg(test)]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// 当前光标字节偏移。
    #[cfg(test)]
    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    /// 追加输入法提交文本。
    pub(crate) fn push_commit(&mut self, text: &str) {
        self.preedit.clear();
        self.insert_str(text);
    }

    /// 更新输入法预编辑文本。
    pub(crate) fn set_preedit(&mut self, text: &str) {
        self.preedit = text.to_string();
    }

    /// 返回合并输入法预编辑后的显示文本。
    pub(crate) fn display_text(&self) -> Cow<'_, str> {
        if self.preedit.is_empty() {
            Cow::Borrowed(&self.text)
        } else {
            let (before, after) = self.text.split_at(self.cursor.min(self.text.len()));
            Cow::Owned(format!("{before}{}{after}", self.preedit))
        }
    }

    /// 返回最终确认文本。
    pub(crate) fn finish_text(&self) -> String {
        self.display_text().into_owned()
    }

    /// 获取有效的选区字节范围 (起始, 结束)。
    pub(crate) fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            None
        } else {
            Some((anchor.min(self.cursor), anchor.max(self.cursor)))
        }
    }

    /// 获取当前选中的纯文本切片。
    pub(crate) fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let safe_start = clamp_boundary(&self.text, start);
        let safe_end = clamp_boundary(&self.text, end);
        if safe_start < safe_end && safe_end <= self.text.len() {
            Some(self.text[safe_start..safe_end].to_string())
        } else {
            None
        }
    }

    /// 若存在选区，将其从文本中删除并重置光标至选区左边界。
    pub(crate) fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection_range() else {
            return false;
        };
        let safe_start = clamp_boundary(&self.text, start);
        let safe_end = clamp_boundary(&self.text, end);
        if safe_start <= safe_end && safe_end <= self.text.len() {
            self.text.replace_range(safe_start..safe_end, "");
            self.cursor = safe_start;
            self.selection_anchor = None;
            self.reset_blink();
            true
        } else {
            self.selection_anchor = None;
            false
        }
    }

    /// 在光标处插入字符（有选区时先替换选区）。
    pub(crate) fn insert_char(&mut self, ch: char) {
        self.delete_selection();
        let safe_cursor = clamp_boundary(&self.text, self.cursor);
        self.text.insert(safe_cursor, ch);
        self.cursor = safe_cursor + ch.len_utf8();
        self.selection_anchor = None;
        self.reset_blink();
    }

    /// 在光标处插入字符串切片（有选区时先替换选区）。
    pub(crate) fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.delete_selection();
        let safe_cursor = clamp_boundary(&self.text, self.cursor);
        self.text.insert_str(safe_cursor, s);
        self.cursor = safe_cursor + s.len();
        self.selection_anchor = None;
        self.reset_blink();
    }

    /// 向前退格删除单个字符（有选区时删除选区）。
    pub(crate) fn pop_char_backward(&mut self) {
        if self.delete_selection() {
            return;
        }
        let safe_cursor = clamp_boundary(&self.text, self.cursor);
        if safe_cursor == 0 {
            return;
        }
        let prev = prev_char_boundary(&self.text, safe_cursor);
        self.text.replace_range(prev..safe_cursor, "");
        self.cursor = prev;
        self.selection_anchor = None;
        self.reset_blink();
    }

    /// 向后 Delete 删除单个字符（有选区时删除选区）。
    pub(crate) fn delete_forward(&mut self) {
        if self.delete_selection() {
            return;
        }
        let safe_cursor = clamp_boundary(&self.text, self.cursor);
        if safe_cursor >= self.text.len() {
            return;
        }
        let next = next_char_boundary(&self.text, safe_cursor);
        self.text.replace_range(safe_cursor..next, "");
        self.selection_anchor = None;
        self.reset_blink();
    }

    /// 光标向左移动一个字符。
    pub(crate) fn move_left(&mut self, extend_selection: bool) {
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
            self.cursor = prev_char_boundary(&self.text, self.cursor);
        } else if let Some((start, _)) = self.selection_range() {
            self.cursor = start;
            self.selection_anchor = None;
        } else {
            self.cursor = prev_char_boundary(&self.text, self.cursor);
            self.selection_anchor = None;
        }
        self.reset_blink();
    }

    /// 光标向右移动一个字符。
    pub(crate) fn move_right(&mut self, extend_selection: bool) {
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
            self.cursor = next_char_boundary(&self.text, self.cursor);
        } else if let Some((_, end)) = self.selection_range() {
            self.cursor = end;
            self.selection_anchor = None;
        } else {
            self.cursor = next_char_boundary(&self.text, self.cursor);
            self.selection_anchor = None;
        }
        self.reset_blink();
    }

    /// 移到行首。
    pub(crate) fn move_home(&mut self, extend_selection: bool) {
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        } else if !extend_selection {
            self.selection_anchor = None;
        }
        self.cursor = find_line_home(&self.text, self.cursor);
        self.reset_blink();
    }

    /// 移到行尾。
    pub(crate) fn move_end(&mut self, extend_selection: bool) {
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        } else if !extend_selection {
            self.selection_anchor = None;
        }
        self.cursor = find_line_end(&self.text, self.cursor);
        self.reset_blink();
    }

    /// 垂直移动光标一行（向上或向下），结合折行排版精准定位目标列。
    pub(crate) fn move_vertical(
        &mut self,
        up: bool,
        extend_selection: bool,
        body_width: f32,
        scale: f32,
    ) {
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }

        let scale = scale.max(0.5);
        let font_size = 14.0 * scale;
        let line_height = 22.0 * scale;
        let padding_x = 12.0 * scale;
        let max_text_width = (body_width - padding_x * 2.0 - 12.0 * scale).max(10.0);
        let options =
            snapin_render::TextMeasureOptions::new(font_size, line_height, max_text_width, 10000.0);

        let text = self.display_text();
        let (new_cursor, cursor_y) = snapin_render::with_thread_local_text_renderer(|renderer| {
            let metrics = renderer.measure_text_cursor(&text, self.cursor, options);
            let target_y = if up {
                (metrics.cursor_y - line_height * 0.5).max(0.0)
            } else {
                metrics.cursor_y + line_height * 1.5
            };
            let offset = renderer.hit_test_text_offset(&text, metrics.cursor_x, target_y, options);
            (offset, metrics.cursor_y)
        });

        self.cursor = clamp_boundary(&self.text, new_cursor);
        self.ensure_cursor_visible(cursor_y, line_height, 300.0 * scale);
        self.reset_blink();
    }

    /// 移到全文首。
    pub(crate) fn move_doc_start(&mut self, extend_selection: bool) {
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        } else if !extend_selection {
            self.selection_anchor = None;
        }
        self.cursor = 0;
        self.reset_blink();
    }

    /// 移到全文末。
    pub(crate) fn move_doc_end(&mut self, extend_selection: bool) {
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        } else if !extend_selection {
            self.selection_anchor = None;
        }
        self.cursor = self.text.len();
        self.reset_blink();
    }

    /// 全选当前所有文本。
    pub(crate) fn select_all(&mut self) {
        if !self.text.is_empty() {
            self.selection_anchor = Some(0);
            self.cursor = self.text.len();
            self.reset_blink();
        }
    }

    /// 清空所有文本与选区。
    pub(crate) fn clear_text(&mut self) {
        self.text.clear();
        self.preedit.clear();
        self.cursor = 0;
        self.selection_anchor = None;
        self.scroll_y = 0.0;
        self.reset_blink();
    }

    /// 一键格式排版整理：去除每行首尾多余空白，并将多余空行压缩为单空行。
    pub(crate) fn format_layout(&mut self) {
        let lines: Vec<&str> = self.text.lines().collect();
        let mut formatted = Vec::with_capacity(lines.len());
        let mut last_was_empty = false;

        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !last_was_empty && !formatted.is_empty() {
                    formatted.push("");
                    last_was_empty = true;
                }
            } else {
                formatted.push(trimmed);
                last_was_empty = false;
            }
        }

        self.text = formatted.join("\n");
        self.cursor = clamp_boundary(&self.text, self.cursor);
        self.selection_anchor = None;
        self.reset_blink();
    }

    /// 统计有效字符总数（以 Unicode Scalar / char 计数）。
    pub(crate) fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    /// 统计行数。
    pub(crate) fn line_count(&self) -> usize {
        if self.text.is_empty() {
            1
        } else {
            self.text.lines().count().max(1)
        }
    }

    /// 滚轮平滑滚动并安全限制在有效范围内。
    pub(crate) fn scroll_by(&mut self, delta_y: f32, body_height: f32) {
        let max_scroll = (self.content_height - body_height).max(0.0);
        self.scroll_y = (self.scroll_y + delta_y).clamp(0.0, max_scroll);
    }

    /// 保证光标在可视区可见（垂直自动滚动）。
    pub(crate) fn ensure_cursor_visible(
        &mut self,
        cursor_y: f32,
        line_height: f32,
        body_height: f32,
    ) {
        if cursor_y < self.scroll_y {
            self.scroll_y = cursor_y.max(0.0);
        } else if cursor_y + line_height > self.scroll_y + body_height {
            self.scroll_y = (cursor_y + line_height - body_height).max(0.0);
        }
    }

    /// 重置光标呼吸闪烁计时器，确保输入时立即处于高亮显示周期。
    pub(crate) fn reset_blink(&mut self) {
        self.blink_start = Instant::now();
    }

    /// 当前光标是否处于闪烁高亮周期。
    pub(crate) fn is_cursor_blink_visible(&self) -> bool {
        (self.blink_start.elapsed().as_millis() % 1000) < 530
    }
}

/// 夹取到最近有效 UTF-8 字符边界。
pub(crate) fn clamp_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// 获取前一个字符边界。
pub(crate) fn prev_char_boundary(text: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let mut prev = cursor - 1;
    while prev > 0 && !text.is_char_boundary(prev) {
        prev -= 1;
    }
    prev
}

/// 获取后一个字符边界。
pub(crate) fn next_char_boundary(text: &str, cursor: usize) -> usize {
    if cursor >= text.len() {
        return text.len();
    }
    let mut next = cursor + 1;
    while next < text.len() && !text.is_char_boundary(next) {
        next += 1;
    }
    next
}

/// 查找行首。
pub(crate) fn find_line_home(text: &str, cursor: usize) -> usize {
    let safe_cursor = clamp_boundary(text, cursor);
    match text[..safe_cursor].rfind('\n') {
        Some(idx) => idx + 1,
        None => 0,
    }
}

/// 查找行尾。
pub(crate) fn find_line_end(text: &str, cursor: usize) -> usize {
    let safe_cursor = clamp_boundary(text, cursor);
    match text[safe_cursor..].find('\n') {
        Some(idx) => safe_cursor + idx,
        None => text.len(),
    }
}
