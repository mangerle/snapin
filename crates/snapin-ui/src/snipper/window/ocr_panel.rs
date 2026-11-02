//! OCR 结果面板状态、布局与导出数据。
//!
//! 面板默认选中所有识别行。用户可以按行取消选择、编辑文本，再将当前选择导出为
//! Markdown 或 JSON。面板只保存文本和布局状态，不执行文件 I/O。

use crate::geometry::Rect;
use crate::ports::OcrTextItem;

/// 面板宽度
const PANEL_WIDTH: f32 = 460.0;
/// 面板距离屏幕边缘的间距
const PANEL_MARGIN: f32 = 16.0;
/// 标题区域高度
const HEADER_HEIGHT: f32 = 52.0;
/// 单行高度
const ROW_HEIGHT: f32 = 28.0;
/// 底部操作提示高度
const FOOTER_HEIGHT: f32 = 52.0;

/// OCR 结果面板中的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrPanelLine {
    /// 当前文本内容
    pub text: String,
    /// 是否参与导出
    pub selected: bool,
}

/// OCR 结果面板状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrPanelState {
    /// 按阅读顺序排列的文本行
    pub lines: Vec<OcrPanelLine>,
    /// 当前键盘焦点行
    pub cursor: usize,
    /// 当前可见区域起始行
    pub scroll_start: usize,
}

/// 面板一次布局计算的结果，渲染与鼠标命中共用。
#[derive(Debug, Clone)]
pub struct OcrPanelLayout {
    /// 面板整体矩形
    pub panel: Rect,
    /// 当前可见行对应的热区
    pub rows: Vec<(usize, Rect)>,
}

impl OcrPanelState {
    /// 从 OCR 文本行创建默认全选面板。
    pub(crate) fn from_ocr_lines(lines: &[OcrTextItem]) -> Self {
        let mut panel_lines = Vec::with_capacity(lines.len());
        for line in lines {
            let text = line.text.trim();
            if !text.is_empty() {
                panel_lines.push(OcrPanelLine {
                    text: text.to_string(),
                    selected: true,
                });
            }
        }
        Self {
            lines: panel_lines,
            cursor: 0,
            scroll_start: 0,
        }
    }

    /// 返回当前选择用于导出的文本行。
    pub(crate) fn selected_lines(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter(|line| line.selected)
            .map(|line| line.text.clone())
            .collect()
    }

    /// 切换当前焦点行的选择状态。
    pub(crate) fn toggle_cursor(&mut self) {
        if let Some(line) = self.lines.get_mut(self.cursor) {
            line.selected = !line.selected;
        }
    }

    /// 全选或取消全选。
    pub(crate) fn toggle_all(&mut self) {
        let select = !self.lines.iter().all(|line| line.selected);
        for line in &mut self.lines {
            line.selected = select;
        }
    }

    /// 上下移动焦点并保持其处于可见区域。
    pub(crate) fn move_cursor(&mut self, delta: isize, visible_rows: usize) {
        if self.lines.is_empty() {
            return;
        }
        let max = self.lines.len().saturating_sub(1);
        self.cursor = if delta < 0 {
            self.cursor.saturating_sub(delta.unsigned_abs())
        } else {
            self.cursor.saturating_add(delta as usize).min(max)
        };
        if self.cursor < self.scroll_start {
            self.scroll_start = self.cursor;
        } else if visible_rows > 0 && self.cursor >= self.scroll_start.saturating_add(visible_rows)
        {
            self.scroll_start = self.cursor.saturating_add(1).saturating_sub(visible_rows);
        }
    }

    /// 用编辑后的文本替换指定行；空文本会保留原文以避免误删。
    pub(crate) fn replace_line(&mut self, index: usize, text: String) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if let Some(line) = self.lines.get_mut(index) {
            line.text = text.to_string();
        }
    }

    /// 计算面板、可见行与鼠标热区。
    pub(crate) fn layout(
        &self,
        screen_width: f32,
        screen_height: f32,
        scale: f32,
    ) -> OcrPanelLayout {
        let width = (PANEL_WIDTH * scale).min(screen_width - PANEL_MARGIN * 2.0);
        let max_rows =
            (((screen_height - HEADER_HEIGHT * scale - FOOTER_HEIGHT * scale - PANEL_MARGIN * 2.0)
                / (ROW_HEIGHT * scale))
                .floor()
                .max(1.0)) as usize;
        let visible_count = self.lines.len().min(max_rows);
        let height = HEADER_HEIGHT * scale
            + ROW_HEIGHT * scale * visible_count as f32
            + FOOTER_HEIGHT * scale;
        let x = (screen_width - width - PANEL_MARGIN * scale).max(PANEL_MARGIN * scale);
        let y = PANEL_MARGIN * scale;
        let rows = (0..visible_count)
            .map(|index| {
                let line_index = self.scroll_start.saturating_add(index);
                let row = Rect {
                    x: x + 10.0 * scale,
                    y: y + HEADER_HEIGHT * scale + index as f32 * ROW_HEIGHT * scale,
                    width: width - 20.0 * scale,
                    height: ROW_HEIGHT * scale,
                };
                (line_index, row)
            })
            .collect();
        OcrPanelLayout {
            panel: Rect {
                x,
                y,
                width,
                height,
            },
            rows,
        }
    }

    /// 返回当前可见行数量。
    pub(crate) fn visible_rows(&self, screen_height: f32, scale: f32) -> usize {
        (((screen_height - HEADER_HEIGHT * scale - FOOTER_HEIGHT * scale - PANEL_MARGIN * 2.0)
            / (ROW_HEIGHT * scale))
            .floor()
            .max(1.0)) as usize
    }

    /// 返回当前焦点行编辑框左上角。
    pub(crate) fn editor_pos(
        &self,
        screen_width: f32,
        screen_height: f32,
        scale: f32,
    ) -> Option<(f32, f32)> {
        let layout = self.layout(screen_width, screen_height, scale);
        layout
            .rows
            .iter()
            .find(|(index, _)| *index == self.cursor)
            .map(|(_, row)| (row.x + 26.0 * scale, row.y + 3.0 * scale))
    }

    /// 处理面板内鼠标点击；命中面板时返回 `true` 以拦截画布交互。
    pub(crate) fn handle_click(&mut self, request: OcrPanelClickRequest) -> bool {
        let layout = self.layout(request.screen_size.0, request.screen_size.1, request.scale);
        let (x, y) = request.point;
        if !layout.panel.contains(x, y) {
            return false;
        }
        if let Some((index, _)) = layout.rows.iter().find(|(_, row)| row.contains(x, y)) {
            self.cursor = *index;
            self.toggle_cursor();
        }
        true
    }
}

/// OCR 面板点击命中参数。
pub(crate) struct OcrPanelClickRequest {
    pub point: (f32, f32),
    pub screen_size: (f32, f32),
    pub scale: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(text: &str) -> OcrTextItem {
        OcrTextItem {
            text: text.to_string(),
            bounds: snapin_foundation::Rect::new(0.0, 0.0, 10.0, 10.0),
        }
    }

    /// 默认全选，取消一行后只导出其余行。
    #[test]
    fn test_selected_lines_follow_row_state() {
        let mut panel = OcrPanelState::from_ocr_lines(&[item("甲"), item("乙")]);
        panel.cursor = 1;
        panel.toggle_cursor();
        assert_eq!(panel.selected_lines(), vec!["甲".to_string()]);
    }

    /// 面板宽度与高度不得超过当前屏幕。
    #[test]
    fn test_layout_stays_inside_small_screen() {
        let panel = OcrPanelState::from_ocr_lines(&[item("甲"), item("乙"), item("丙")]);
        let layout = panel.layout(360.0, 240.0, 1.0);
        assert!(layout.panel.x >= 0.0);
        assert!(layout.panel.x + layout.panel.width <= 360.0 + 0.01);
        assert!(layout.panel.y + layout.panel.height <= 240.0 + 0.01);
    }
}
