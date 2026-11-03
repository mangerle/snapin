//! 文字识别全文编辑框状态与绘制。

use crate::drawing::{add_round_rect, make_paint, RoundRect};
use crate::geometry::Rect;
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions};
use std::borrow::Cow;
use tiny_skia::{Color, Pixmap, PixmapMut, PixmapPaint, Stroke, Transform};

/// 面板宽度基准值。
const PANEL_WIDTH: f32 = 640.0;
/// 面板高度基准值。
const PANEL_HEIGHT: f32 = 480.0;
/// 面板与屏幕边缘的最小间距。
const SCREEN_MARGIN: f32 = 16.0;
/// 标题区域高度。
const HEADER_HEIGHT: f32 = 42.0;
/// 底部提示区域高度。
const FOOTER_HEIGHT: f32 = 34.0;

/// 文字识别全文编辑状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcrTextEditorState {
    text: String,
    preedit: String,
}

impl OcrTextEditorState {
    /// 使用 OCR 全文创建编辑框。
    pub(crate) fn new(text: String) -> Self {
        Self {
            text,
            preedit: String::new(),
        }
    }

    /// 当前编辑内容，供测试与提交读取。
    #[cfg(test)]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// 追加输入法提交文本。
    pub(crate) fn push_commit(&mut self, text: &str) {
        self.preedit.clear();
        self.text.push_str(text);
    }

    /// 更新输入法预编辑文本。
    pub(crate) fn set_preedit(&mut self, text: &str) {
        self.preedit = text.to_string();
    }

    /// 追加普通字符、空格或换行。
    pub(crate) fn push_str(&mut self, text: &str) {
        self.text.push_str(text);
    }

    /// 删除末尾字符；预编辑存在时优先删除预编辑内容。
    pub(crate) fn pop_char(&mut self) {
        if self.preedit.pop().is_none() {
            self.text.pop();
        }
    }

    /// 返回合并预编辑内容后的完整文本。
    pub(crate) fn finish_text(&self) -> String {
        if self.preedit.is_empty() {
            self.text.clone()
        } else {
            format!("{}{}", self.text, self.preedit)
        }
    }

    /// 计算编辑框、正文区域与输入法光标位置。
    pub(crate) fn layout(
        &self,
        screen_width: f32,
        screen_height: f32,
        scale: f32,
    ) -> OcrEditorLayout {
        let scale = scale.max(0.5);
        let width =
            (PANEL_WIDTH * scale).min((screen_width - SCREEN_MARGIN * 2.0 * scale).max(1.0));
        let height =
            (PANEL_HEIGHT * scale).min((screen_height - SCREEN_MARGIN * 2.0 * scale).max(1.0));
        let panel = Rect {
            x: (screen_width - width) / 2.0,
            y: (screen_height - height) / 2.0,
            width,
            height,
        };
        let padding = 16.0 * scale;
        let body = Rect {
            x: panel.x + padding,
            y: panel.y + HEADER_HEIGHT * scale,
            width: (panel.width - padding * 2.0).max(1.0),
            height: (panel.height - HEADER_HEIGHT * scale - FOOTER_HEIGHT * scale - padding)
                .max(1.0),
        };
        OcrEditorLayout { panel, body, scale }
    }

    /// 绘制全文编辑框；正文先渲染到独立位图，确保长文本不会越出编辑区域。
    pub(crate) fn render(&self, pixmap: &mut PixmapMut, scale: f32) {
        let layout = self.layout(pixmap.width() as f32, pixmap.height() as f32, scale);
        render_panel_surface(pixmap, layout);
        render_body(pixmap, layout, self.display_text().as_ref());
        render_labels(pixmap, layout);
    }

    fn display_text(&self) -> Cow<'_, str> {
        if self.preedit.is_empty() {
            Cow::Borrowed(&self.text)
        } else {
            Cow::Owned(format!("{}{}", self.text, self.preedit))
        }
    }
}

/// 编辑框布局结果。
#[derive(Debug, Clone, Copy)]
pub(crate) struct OcrEditorLayout {
    pub(crate) panel: Rect,
    pub(crate) body: Rect,
    pub(crate) scale: f32,
}

/// 绘制编辑框底板与边框。
fn render_panel_surface(pixmap: &mut PixmapMut, layout: OcrEditorLayout) {
    let mut panel = tiny_skia::PathBuilder::new();
    add_round_rect(
        &mut panel,
        RoundRect {
            x: layout.panel.x,
            y: layout.panel.y,
            width: layout.panel.width,
            height: layout.panel.height,
            radius: 10.0 * layout.scale,
        },
    );
    let Some(path) = panel.finish() else {
        return;
    };
    pixmap.fill_path(
        &path,
        &make_paint(Color::from_rgba8(248, 250, 252, 250)),
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    pixmap.stroke_path(
        &path,
        &make_paint(Color::from_rgba8(148, 163, 184, 245)),
        &Stroke {
            width: layout.scale,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
}

/// 绘制标题与快捷键提示。
fn render_labels(pixmap: &mut PixmapMut, layout: OcrEditorLayout) {
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            snapin_foundation::i18n::tr("编辑识别文字"),
            TextDrawOptions::new(
                layout.panel.x + 16.0 * layout.scale,
                layout.panel.y + 12.0 * layout.scale,
                16.0 * layout.scale,
                (15, 23, 42, 255),
            ),
        );
        renderer.draw_text_rgba(
            pixmap,
            snapin_foundation::i18n::tr("Ctrl+Enter 复制并关闭  Esc 取消"),
            TextDrawOptions::new(
                layout.panel.x + 16.0 * layout.scale,
                layout.panel.y + layout.panel.height - 26.0 * layout.scale,
                12.0 * layout.scale,
                (100, 116, 139, 255),
            ),
        );
    });
}

fn render_body(pixmap: &mut PixmapMut, layout: OcrEditorLayout, text: &str) {
    let body_width = layout.body.width.max(1.0) as u32;
    let body_height = layout.body.height.max(1.0) as u32;
    let Some(mut body) = Pixmap::new(body_width, body_height) else {
        return;
    };
    body.fill(Color::from_rgba8(255, 255, 255, 255));
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            &mut body.as_mut(),
            text,
            TextDrawOptions::new(
                8.0 * layout.scale,
                8.0 * layout.scale,
                14.0 * layout.scale,
                (30, 41, 59, 255),
            ),
        );
    });
    pixmap.draw_pixmap(
        layout.body.x as i32,
        layout.body.y as i32,
        body.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    /// 编辑框必须保留完整 OCR 文本，并支持换行与退格。
    #[test]
    fn test_ocr_editor_preserves_and_edits_full_text() {
        let mut editor = OcrTextEditorState::new("第一行\n第二行".to_string());
        editor.push_str("\n第三行");
        editor.pop_char();
        assert_eq!(editor.text(), "第一行\n第二行\n第三");
        editor.push_commit("行");
        assert_eq!(editor.finish_text(), "第一行\n第二行\n第三行");
    }

    /// 编辑框必须实际绘制可见面板和正文。
    #[test]
    fn test_ocr_editor_renders_visible_pixels() {
        let editor = OcrTextEditorState::new("可见的识别文字".to_string());
        let mut pixmap = Pixmap::new(800, 600).expect("测试位图应可分配");
        pixmap.fill(Color::TRANSPARENT);
        editor.render(&mut pixmap.as_mut(), 1.0);
        assert!(pixmap
            .data()
            .iter()
            .skip(3)
            .step_by(4)
            .any(|alpha| *alpha > 0));
    }
}
