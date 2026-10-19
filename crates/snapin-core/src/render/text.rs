//! 纯 Rust 高精度文本排版与光栅化渲染服务。
//!
//! 基于 cosmic-text 提供统一的多语言中英文字体加载、高精度排版与 Alpha 混合像素流转，
//! 并提供线程局部缓存以避免在非主干上下文内重复扫描与初始化系统字体库。

use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, FontSystem, Metrics, Shaping, SwashCache, Weight,
};
use std::cell::RefCell;
use tiny_skia::{Color, PixmapMut};

thread_local! {
    static THREAD_LOCAL_TEXT_RENDERER: RefCell<TextRenderer> = RefCell::new(TextRenderer::new());
}

/// 纯 Rust 高精度文本渲染上下文
pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRenderer {
    /// 构造新的文本渲染器并初始化字体缓存
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        }
    }

    /// 在指定坐标绘制中文或英文排版文字（使用 tiny_skia::Color）
    pub fn draw_text(
        &mut self,
        pixmap: &mut PixmapMut,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
    ) {
        let rgba = (
            (color.red() * 255.0).round() as u8,
            (color.green() * 255.0).round() as u8,
            (color.blue() * 255.0).round() as u8,
            (color.alpha() * 255.0).round() as u8,
        );
        self.draw_text_rgba(pixmap, text, x, y, font_size, rgba);
    }

    /// 在指定坐标绘制中文或英文排版文字（使用 RGBA 原始字节四元组）
    pub fn draw_text_rgba(
        &mut self,
        pixmap: &mut PixmapMut,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        rgba: (u8, u8, u8, u8),
    ) {
        if text.is_empty() {
            return;
        }

        let metrics = Metrics::new(font_size, font_size * 1.25);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(
            &mut self.font_system,
            pixmap.width() as f32,
            pixmap.height() as f32,
        );
        buffer.set_text(&mut self.font_system, text, Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let c_color = CosmicColor::rgba(rgba.0, rgba.1, rgba.2, rgba.3);
        let width = pixmap.width() as i32;
        let height = pixmap.height() as i32;

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            c_color,
            |gx, gy, _w, _h, c| {
                let px = x as i32 + gx;
                let py = y as i32 + gy;
                if px >= 0 && px < width && py >= 0 && py < height {
                    let idx = ((py as usize) * (width as usize) + (px as usize)) * 4;
                    blend_glyph_rgba(pixmap.data_mut(), idx, c.r(), c.g(), c.b(), c.a());
                }
            },
        );
    }

    /// 在气泡中心高精排版绘制加粗现代无衬线数字并实现光学绝对居中
    pub fn draw_badge_digit(
        &mut self,
        pixmap: &mut PixmapMut,
        cx: f32,
        cy: f32,
        num: usize,
        r: f32,
    ) {
        let text = num.to_string();
        let font_size = if num < 10 {
            r * 1.35
        } else if num < 100 {
            r * 1.05
        } else {
            r * 0.8
        };

        let metrics = Metrics::new(font_size, font_size * 1.25);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, r * 4.0, r * 4.0);
        buffer.set_text(
            &mut self.font_system,
            &text,
            Attrs::new().weight(Weight::BOLD),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        // 先预计算所有字形包围盒，确保精准几何光学居中
        let mut min_gx = f32::MAX;
        let mut max_gx = f32::MIN;
        let mut min_gy = f32::MAX;
        let mut max_gy = f32::MIN;
        let mut has_glyphs = false;

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            CosmicColor::rgba(255, 255, 255, 255),
            |gx, gy, w, h, _c| {
                if w > 0 && h > 0 {
                    has_glyphs = true;
                    min_gx = min_gx.min(gx as f32);
                    max_gx = max_gx.max((gx + w as i32) as f32);
                    min_gy = min_gy.min(gy as f32);
                    max_gy = max_gy.max((gy + h as i32) as f32);
                }
            },
        );

        let (offset_x, offset_y) = if has_glyphs {
            let text_center_x = (min_gx + max_gx) / 2.0;
            let text_center_y = (min_gy + max_gy) / 2.0;
            (cx - text_center_x, cy - text_center_y)
        } else {
            (cx - font_size * 0.3, cy - font_size * 0.5)
        };

        let width = pixmap.width() as i32;
        let height = pixmap.height() as i32;

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            CosmicColor::rgba(255, 255, 255, 255),
            |gx, gy, _w, _h, c| {
                let px = offset_x as i32 + gx;
                let py = offset_y as i32 + gy;
                if px >= 0 && px < width && py >= 0 && py < height {
                    let idx = ((py as usize) * (width as usize) + (px as usize)) * 4;
                    blend_glyph_rgba(pixmap.data_mut(), idx, c.r(), c.g(), c.b(), c.a());
                }
            },
        );
    }

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

/// 借用线程局部文本渲染器执行闭包，避免在临时标注图元中反复分配字体引擎
pub fn with_thread_local_text_renderer<R>(f: impl FnOnce(&mut TextRenderer) -> R) -> R {
    THREAD_LOCAL_TEXT_RENDERER.with(|cell| {
        let mut renderer = cell.borrow_mut();
        f(&mut renderer)
    })
}

/// 将单个字形像素通过 Alpha 混合写入目标 RGBA 缓冲区
#[inline]
fn blend_glyph_rgba(data: &mut [u8], idx: usize, r: u8, g: u8, b: u8, a: u8) {
    let src_a = a as f32 / 255.0;
    let dst_a = data[idx + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a > 0.0 {
        let src_r = r as f32 * src_a;
        let src_g = g as f32 * src_a;
        let src_b = b as f32 * src_a;
        let dst_r = data[idx] as f32 * dst_a;
        let dst_g = data[idx + 1] as f32 * dst_a;
        let dst_b = data[idx + 2] as f32 * dst_a;
        data[idx] = ((src_r + dst_r * (1.0 - src_a)) / out_a) as u8;
        data[idx + 1] = ((src_g + dst_g * (1.0 - src_a)) / out_a) as u8;
        data[idx + 2] = ((src_b + dst_b * (1.0 - src_a)) / out_a) as u8;
        data[idx + 3] = (out_a * 255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    #[test]
    fn test_text_renderer_basic_draw() {
        let mut pixmap = Pixmap::new(100, 40).unwrap();
        let mut renderer = TextRenderer::new();
        renderer.draw_text(
            &mut pixmap.as_mut(),
            "Snapin",
            10.0,
            10.0,
            16.0,
            Color::from_rgba8(255, 255, 255, 255),
        );
        let has_pixel = pixmap.data().iter().any(|&b| b > 0);
        assert!(has_pixel, "排版绘制应写入非零像素");
    }

    #[test]
    fn test_thread_local_text_renderer() {
        let mut pixmap = Pixmap::new(50, 50).unwrap();
        with_thread_local_text_renderer(|renderer| {
            renderer.draw_badge_digit(&mut pixmap.as_mut(), 25.0, 25.0, 1, 15.0);
        });
        let has_pixel = pixmap.data().iter().any(|&b| b > 0);
        assert!(has_pixel, "标号数字排版绘制应写入非零像素");
    }
}
