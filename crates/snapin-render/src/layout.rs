//! 文本排版：包围盒折行、盒内裁剪落笔与居中加粗文本。

use super::raster::{blend_glyph_rgba, GlyphPixel};
use super::text::{TextBoxOptions, TextRenderer};
use cosmic_text::{Attrs, Buffer, Color as CosmicColor, Metrics, Shaping, Weight};
use tiny_skia::PixmapMut;

/// 居中加粗文本的几何与样式参数。
#[derive(Debug, Clone, Copy)]
pub struct CenteredTextOptions {
    /// 文本光学中心
    pub center: (f32, f32),
    /// 字号
    pub font_size: f32,
    /// 排版可用边长
    pub max_extent: f32,
    /// 文本颜色
    pub rgba: (u8, u8, u8, u8),
}

impl TextRenderer {
    /// 按包围盒宽度完成折行排版，返回可供测量的就绪缓冲区。
    ///
    /// 排版视口高度取 `box_h` 与单行行高的较大值：cosmic-text 只会排布视口内可见的行，
    /// 若直接用 `box_h` 作为视口高度，当盒高不足一行时会排布出零行文本并绘制成空白，
    /// 表现为“翻译/标注文字莫名消失”。保底一行既保证始终有可见字形，
    /// 又让溢出判断自然收敛（total_text_h 至少为一个行高，必然高于不足的盒高）。
    pub(super) fn shape_box_text(
        &mut self,
        text: &str,
        font_size: f32,
        line_height: f32,
        box_w: f32,
        box_h: f32,
    ) -> Buffer {
        let metrics = Metrics::new(font_size, line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, box_w, box_h.max(line_height));
        buffer.set_text(&mut self.font_system, text, Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }

    /// 将排版完成的缓冲区落笔到包围盒内，并把字形像素严格裁剪在盒形区域。
    ///
    /// 裁剪同时受盒形范围与画布物理边界约束：前者保证溢出文本不外溢，
    /// 后者保证索引换算不会越出像素缓冲区。
    pub(super) fn paint_buffer_in_box(
        &mut self,
        pixmap: &mut PixmapMut,
        buffer: &mut Buffer,
        options: &TextBoxOptions,
        vertical_offset: f32,
    ) {
        let (x, y, box_w, box_h) = options.rect;
        let color = CosmicColor::rgba(
            options.rgba.0,
            options.rgba.1,
            options.rgba.2,
            options.rgba.3,
        );
        let width = pixmap.width() as i32;
        let height = pixmap.height() as i32;
        let origin_x = x as i32;
        let origin_y = (y + vertical_offset) as i32;
        let max_x = (x + box_w).floor() as i32;
        let max_y = (y + box_h).floor() as i32;

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            color,
            |gx, gy, _w, _h, c| {
                let px = origin_x + gx;
                let py = origin_y + gy;
                let inside_box = px >= origin_x && px < max_x && py >= origin_y && py < max_y;
                let inside_canvas = px >= 0 && px < width && py >= 0 && py < height;
                if inside_box && inside_canvas {
                    let idx = ((py as usize) * (width as usize) + (px as usize)) * 4;
                    blend_glyph_rgba(
                        pixmap.data_mut(),
                        idx,
                        GlyphPixel {
                            red: c.r(),
                            green: c.g(),
                            blue: c.b(),
                            alpha: c.a(),
                        },
                    );
                }
            },
        );
    }

    /// 在指定中心点绘制加粗文本，并按实际字形包围盒做光学居中。
    ///
    /// 该方法只处理通用文本几何，不理解“徽标/序号”等业务语义；
    /// 字号与可用尺寸由调用方的图元模块决定。
    pub fn draw_centered_bold_text(
        &mut self,
        pixmap: &mut PixmapMut,
        text: &str,
        options: CenteredTextOptions,
    ) {
        let CenteredTextOptions {
            center: (cx, cy),
            font_size,
            max_extent,
            rgba,
        } = options;
        let color = CosmicColor::rgba(rgba.0, rgba.1, rgba.2, rgba.3);
        let mut buffer = self.shape_centered_bold_text(text, font_size, max_extent);
        let bounds = self.measure_visible_glyph_bounds(&mut buffer, color);
        let offset = centered_offset(bounds, font_size, (cx, cy));
        self.paint_centered_buffer(pixmap, &mut buffer, CenteredPaint { offset, color });
    }

    /// 按居中文本参数完成粗体排版。
    fn shape_centered_bold_text(&mut self, text: &str, font_size: f32, max_extent: f32) -> Buffer {
        let metrics = Metrics::new(font_size, font_size * 1.25);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, max_extent, max_extent);
        buffer.set_text(
            &mut self.font_system,
            text,
            Attrs::new().weight(Weight::BOLD),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }

    /// 预计算所有可见字形的包围盒，确保精准几何光学居中。
    fn measure_visible_glyph_bounds(
        &mut self,
        buffer: &mut Buffer,
        color: CosmicColor,
    ) -> Option<GlyphBounds> {
        let mut min_gx = f32::MAX;
        let mut max_gx = f32::MIN;
        let mut min_gy = f32::MAX;
        let mut max_gy = f32::MIN;
        let mut has_glyphs = false;

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            color,
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
        has_glyphs.then_some(GlyphBounds {
            min_x: min_gx,
            max_x: max_gx,
            min_y: min_gy,
            max_y: max_gy,
        })
    }

    /// 按字形包围盒中心将排版结果落笔到目标画布。
    fn paint_centered_buffer(
        &mut self,
        pixmap: &mut PixmapMut,
        buffer: &mut Buffer,
        paint: CenteredPaint,
    ) {
        let width = pixmap.width() as i32;
        let height = pixmap.height() as i32;
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            paint.color,
            |gx, gy, _w, _h, c| {
                let px = paint.offset.0 as i32 + gx;
                let py = paint.offset.1 as i32 + gy;
                if px >= 0 && px < width && py >= 0 && py < height {
                    let idx = ((py as usize) * (width as usize) + (px as usize)) * 4;
                    blend_glyph_rgba(
                        pixmap.data_mut(),
                        idx,
                        GlyphPixel {
                            red: c.r(),
                            green: c.g(),
                            blue: c.b(),
                            alpha: c.a(),
                        },
                    );
                }
            },
        );
    }
}

/// 居中文本的最终落笔偏移与颜色。
#[derive(Debug, Clone, Copy)]
struct CenteredPaint {
    offset: (f32, f32),
    color: CosmicColor,
}

/// 可见字形的设备像素包围盒。
#[derive(Debug, Clone, Copy)]
struct GlyphBounds {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

/// 根据字形包围盒计算相对目标中心的偏移。
fn centered_offset(bounds: Option<GlyphBounds>, font_size: f32, center: (f32, f32)) -> (f32, f32) {
    let Some(bounds) = bounds else {
        return (center.0 - font_size * 0.3, center.1 - font_size * 0.5);
    };
    let text_center_x = (bounds.min_x + bounds.max_x) / 2.0;
    let text_center_y = (bounds.min_y + bounds.max_y) / 2.0;
    (center.0 - text_center_x, center.1 - text_center_y)
}
