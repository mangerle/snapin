//! 文字识别全文编辑框模块定义与入口。

pub(crate) mod layout;
pub(crate) mod render;
pub(crate) mod state;
#[cfg(test)]
mod tests;

pub(crate) use layout::{compute_editor_layout, OcrEditorButton, OcrEditorLayout, ScrollbarHit};
pub(crate) use render::render_ocr_editor;
pub(crate) use state::OcrTextEditorState;

impl OcrTextEditorState {
    /// 计算编辑框完整排版布局。
    pub(crate) fn layout(
        &self,
        screen_width: f32,
        screen_height: f32,
        scale: f32,
    ) -> OcrEditorLayout {
        compute_editor_layout(self, screen_width, screen_height, scale)
    }

    /// 绘制全文编辑框。
    pub(crate) fn render(&self, pixmap: &mut tiny_skia::PixmapMut, scale: f32) {
        let layout = self.layout(pixmap.width() as f32, pixmap.height() as f32, scale);
        render_ocr_editor(self, pixmap, &layout);
    }

    /// 测量并更新当前文本排版总高度，确保滚动范围精确匹配。
    pub(crate) fn update_content_metrics(&mut self, body_width: f32, scale: f32) {
        let scale = scale.max(0.5);
        let font_size = 14.0 * scale;
        let line_height = 22.0 * scale;
        let padding_x = 12.0 * scale;
        let max_text_width = (body_width - padding_x * 2.0 - 12.0 * scale).max(10.0);
        let options =
            snapin_render::TextMeasureOptions::new(font_size, line_height, max_text_width, 10000.0);
        let text = self.display_text().into_owned();
        let total_height = snapin_render::with_thread_local_text_renderer(|renderer| {
            let metrics = renderer.measure_text_input(&text, options);
            metrics.total_height
        });
        self.content_height = total_height + 20.0 * scale;
    }
}
