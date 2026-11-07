//! 文本输入会话核心状态与字符增删选区维护。

use super::cursor::{find_line_end, find_line_home, next_char_boundary, prev_char_boundary};
use crate::geometry::Point;
use crate::toolbar::SubToolOptions;
use snapin_annotation::action::AnnotationShape;
use snapin_annotation::shapes::TextAction;
use snapin_render::{with_thread_local_text_renderer, TextMeasureOptions};
use tiny_skia::Color;

/// 正在进行的文本标注交互输入会话
#[derive(Debug, Clone)]
pub struct TextInputSession {
    /// 文本起始左上角坐标 (x, y)
    pub pos: Point,
    /// 已提交确认的文本字符内容
    pub text: String,
    /// 候选输入法预编辑拼音内容 (IME Preedit)
    pub preedit: String,
    /// 当前光标在 text 中的 UTF-8 字节偏移
    pub cursor: usize,
    /// 选区起始锚点；若存在且与 cursor 不同则形成高亮选中范围
    pub selection_anchor: Option<usize>,
    /// 文字颜色 (r, g, b, a)
    pub color: (u8, u8, u8, u8),
    /// 字体字号大小 (范围 12.0 ~ 72.0)
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub font_family: Option<String>,
    pub stroke_width: f32,
    pub stroke_color: (u8, u8, u8, u8),
    pub background_color: Option<(u8, u8, u8, u8)>,
    pub background_opacity: u8,
    pub background_padding: f32,
    pub background_corner_radius: f32,
    /// 触发就地重新编辑时的原始图元备份
    pub original_shape: Option<AnnotationShape>,
}

impl TextInputSession {
    /// 创建新的文本输入会话
    pub fn new(pos: (f32, f32), color: (u8, u8, u8, u8), font_size: f32) -> Self {
        Self {
            pos: Point::from(pos),
            text: String::with_capacity(64),
            preedit: String::new(),
            cursor: 0,
            selection_anchor: None,
            color,
            font_size: font_size.clamp(12.0, 72.0),
            bold: false,
            italic: false,
            font_family: None,
            stroke_width: 0.0,
            stroke_color: (255, 255, 255, 255),
            background_color: None,
            background_opacity: 0,
            background_padding: 4.0,
            background_corner_radius: 4.0,
            original_shape: None,
        }
    }

    /// 从已有的持久化文本标注恢复为二次编辑会话，光标默认置于末尾
    pub fn from_text_action(action: TextAction) -> Self {
        let original_shape = Some(AnnotationShape::Text(action.clone()));
        let cursor = action.text.len();
        Self {
            pos: action.pos,
            text: action.text,
            preedit: String::new(),
            cursor,
            selection_anchor: None,
            color: action.color.to_tuple(),
            font_size: action.font_size,
            bold: action.bold,
            italic: action.italic,
            font_family: action.font_family,
            stroke_width: action.stroke_width,
            stroke_color: action.stroke_color.to_tuple(),
            background_color: action
                .background_color
                .map(snapin_annotation::ColorRgba::to_tuple),
            background_opacity: action.background_opacity,
            background_padding: action.padding,
            background_corner_radius: action.corner_radius,
            original_shape,
        }
    }

    /// 当前选区范围 (start, end)，保证 start < end
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            None
        } else {
            Some((anchor.min(self.cursor), anchor.max(self.cursor)))
        }
    }

    /// 获取当前选中的文本切片
    pub fn selected_text(&self) -> Option<&str> {
        let (start, end) = self.selection_range()?;
        self.text.get(start..end)
    }

    /// 清空选区状态
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// 全选当前文本
    pub fn select_all(&mut self) {
        if !self.text.is_empty() {
            self.selection_anchor = Some(0);
            self.cursor = self.text.len();
        }
    }

    /// 删除选中的文本区间；若无选区则返回 false
    pub fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection_range() else {
            return false;
        };
        self.text.drain(start..end);
        self.cursor = start;
        self.selection_anchor = None;
        true
    }

    /// 在当前光标处插入字符（若有选区则先删除选区）
    pub fn insert_char(&mut self, ch: char) {
        self.delete_selection();
        self.ensure_cursor_valid();
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    /// 在当前光标处插入字符串切片（若有选区则先删除选区）
    pub fn insert_str(&mut self, s: &str) {
        self.delete_selection();
        self.ensure_cursor_valid();
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// 追加单个字符（仅供测试快速追加）
    #[cfg(test)]
    pub fn push_char(&mut self, ch: char) {
        self.insert_char(ch);
    }

    /// 追加字符串切片并移动光标。
    pub fn push_str(&mut self, s: &str) {
        self.insert_str(s);
    }

    /// 接收输入法确认提交的中文/外文文本
    pub fn push_commit(&mut self, s: &str) {
        self.preedit.clear();
        self.insert_str(s);
    }

    /// 接收输入法候选状态变动
    pub fn set_preedit(&mut self, s: &str) {
        self.preedit = s.to_string();
    }

    /// 向后退格删除（Backspace）
    pub fn delete_backward(&mut self) {
        if !self.preedit.is_empty() {
            self.preedit.pop();
            return;
        }
        if self.delete_selection() {
            return;
        }
        if self.cursor > 0 {
            let prev = prev_char_boundary(&self.text, self.cursor);
            self.text.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    /// 向前删除字符（Delete）
    pub fn delete_forward(&mut self) {
        if !self.preedit.is_empty() {
            self.preedit.clear();
            return;
        }
        if self.delete_selection() {
            return;
        }
        if self.cursor < self.text.len() {
            let next = next_char_boundary(&self.text, self.cursor);
            self.text.drain(self.cursor..next);
        }
    }

    /// 删除末尾字符（仅供测试验证）
    #[cfg(test)]
    pub fn pop_char(&mut self) {
        self.delete_backward();
    }

    /// 光标向左移动一个字符
    pub fn move_cursor_left(&mut self, select: bool) {
        self.update_selection_on_move(select);
        self.cursor = prev_char_boundary(&self.text, self.cursor);
    }

    /// 光标向右移动一个字符
    pub fn move_cursor_right(&mut self, select: bool) {
        self.update_selection_on_move(select);
        self.cursor = next_char_boundary(&self.text, self.cursor);
    }

    /// 光标移动到当前行首
    pub fn move_cursor_home(&mut self, select: bool) {
        self.update_selection_on_move(select);
        self.cursor = find_line_home(&self.text, self.cursor);
    }

    /// 光标移动到当前行尾
    pub fn move_cursor_end(&mut self, select: bool) {
        self.update_selection_on_move(select);
        self.cursor = find_line_end(&self.text, self.cursor);
    }

    /// 光标向上移动一行（基于排版实际坐标定位对应水平位置）
    pub fn move_cursor_up(&mut self, select: bool) {
        self.update_selection_on_move(select);
        let options = self.measure_options(1.0);
        let (cur_x, cur_y) = self.measure_cursor_pos(options);
        if cur_y <= 0.0 {
            self.cursor = 0;
            return;
        }
        let target_y = (cur_y - options.line_height * 0.5).max(0.0);
        with_thread_local_text_renderer(|renderer| {
            self.cursor = renderer.hit_test_text_offset(&self.text, cur_x, target_y, options);
        });
    }

    /// 光标向下移动一行（基于排版实际坐标定位对应水平位置）
    pub fn move_cursor_down(&mut self, select: bool) {
        self.update_selection_on_move(select);
        let options = self.measure_options(1.0);
        let (cur_x, cur_y) = self.measure_cursor_pos(options);
        let target_y = cur_y + options.line_height * 1.5;
        with_thread_local_text_renderer(|renderer| {
            self.cursor = renderer.hit_test_text_offset(&self.text, cur_x, target_y, options);
        });
    }

    /// 根据屏幕点击点重设光标位置，可选保留选区
    pub fn set_cursor_from_screen_point(
        &mut self,
        screen_x: f32,
        screen_y: f32,
        select: bool,
        scale: f32,
    ) {
        self.update_selection_on_move(select);
        let scale = if scale > f32::EPSILON { scale } else { 1.0 };
        let hit_x = (screen_x - self.pos.x * scale).max(0.0);
        let hit_y = (screen_y - self.pos.y * scale).max(0.0);
        let options = self.measure_options(scale);
        with_thread_local_text_renderer(|renderer| {
            self.cursor = renderer.hit_test_text_offset(&self.text, hit_x, hit_y, options);
        });
    }

    /// 判定屏幕坐标点是否落在当前输入框外接矩形内
    pub fn contains_screen_point(&self, screen_x: f32, screen_y: f32, scale: f32) -> bool {
        let (bx, by, bw, bh) = self.input_box_rect_scaled(scale);
        screen_x >= bx && screen_x <= bx + bw && screen_y >= by && screen_y <= by + bh
    }

    /// 获取缩放后输入框的屏幕外接矩形 (x, y, width, height)
    pub fn input_box_rect_scaled(&self, scale: f32) -> (f32, f32, f32, f32) {
        let scale = if scale > f32::EPSILON { scale } else { 1.0 };
        let (x, y) = (self.pos.x * scale, self.pos.y * scale);
        let font_size = (self.font_size * scale).max(8.0 * scale.min(1.0));
        let line_height = font_size * 1.25;

        let options = TextMeasureOptions {
            font_size,
            line_height,
            max_width: 8192.0,
            max_height: 8192.0,
        };

        let (max_w, total_h) = with_thread_local_text_renderer(|renderer| {
            let m = renderer.measure_text_cursor(&self.text, self.cursor, options);
            (m.max_line_width, m.total_height)
        });

        let min_w = (font_size * 3.0).max(64.0 * scale);
        let box_w = (max_w + 24.0 * scale).max(min_w);
        let box_h = total_h + 12.0 * scale;

        (x - 8.0 * scale, y - 6.0 * scale, box_w, box_h)
    }

    /// 辅助方法：移动时维护选区锚点
    fn update_selection_on_move(&mut self, select: bool) {
        if select {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.clear_selection();
        }
    }

    /// 确保当前光标位于合法的 UTF-8 字符边界
    fn ensure_cursor_valid(&mut self) {
        self.cursor = self.cursor.min(self.text.len());
        while self.cursor > 0 && !self.text.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }
    }

    /// 构造测量参数
    pub(crate) fn measure_options(&self, scale: f32) -> TextMeasureOptions {
        let scale = if scale > f32::EPSILON { scale } else { 1.0 };
        let font_size = (self.font_size * scale).max(8.0 * scale.min(1.0));
        let line_height = font_size * 1.25;
        TextMeasureOptions {
            font_size,
            line_height,
            max_width: 8192.0,
            max_height: 8192.0,
        }
    }

    /// 测量当前光标相对输入框左上角的相对坐标 (cursor_x, cursor_y)
    fn measure_cursor_pos(&self, options: TextMeasureOptions) -> (f32, f32) {
        with_thread_local_text_renderer(|renderer| {
            let m = renderer.measure_text_cursor(&self.text, self.cursor, options);
            (m.cursor_x, m.cursor_y)
        })
    }

    /// 动态调节字号，仅供单元测试验证边界行为
    #[cfg(test)]
    pub(crate) fn adjust_font_size(&mut self, delta: f32) {
        self.font_size = (self.font_size + delta).clamp(12.0, 72.0);
    }

    /// 将工具栏当前文字样式应用到会话
    pub fn apply_toolbar_options(&mut self, options: &SubToolOptions) {
        self.color = color_to_rgba(options.color);
        self.font_size = options.font_size;
        self.bold = options.text_bold;
        self.italic = options.text_italic;
        self.font_family = options.font_family.map(str::to_string);
        self.stroke_width = options.text_stroke_width;
        // 描边与文字颜色保持清晰对比，若未启用独立配置则默认高对比度白描边
        self.stroke_color = if self.color.0 > 200 && self.color.1 > 200 && self.color.2 > 200 {
            (15, 23, 42, 255)
        } else {
            (255, 255, 255, 255)
        };
        self.background_color = options.text_background_enabled.then_some(self.color);
        self.background_opacity = if options.text_background_enabled {
            options.text_background_alpha()
        } else {
            0
        };
        self.background_padding = options.text_background_padding;
        self.background_corner_radius = options.text_background_corner_radius;
    }

    /// 将当前会话转换为持久化标注动作
    pub fn commit(mut self) -> Option<AnnotationShape> {
        if !self.preedit.is_empty() {
            self.text.insert_str(self.cursor, &self.preedit);
            self.preedit.clear();
        }
        let trimmed = self.text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(AnnotationShape::Text(TextAction {
                pos: self.pos,
                text: self.text,
                color: self.color.into(),
                font_size: self.font_size,
                bold: self.bold,
                italic: self.italic,
                font_family: self.font_family,
                stroke_width: self.stroke_width,
                stroke_color: self.stroke_color.into(),
                background_color: self.background_color.map(Into::into),
                background_opacity: self.background_opacity,
                padding: self.background_padding,
                corner_radius: self.background_corner_radius,
            }))
        }
    }
}

/// 将 tiny-skia 颜色转换为 RGBA 元组
fn color_to_rgba(color: Color) -> (u8, u8, u8, u8) {
    (
        (color.red() * 255.0).round() as u8,
        (color.green() * 255.0).round() as u8,
        (color.blue() * 255.0).round() as u8,
        (color.alpha() * 255.0).round() as u8,
    )
}
