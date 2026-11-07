//! 纯 Rust 高精度文本排版与光栅化渲染服务。
//!
//! 基于 cosmic-text 提供统一的多语言中英文字体加载、高精度排版与 Alpha 混合像素流转，
//! 并提供线程局部缓存以避免在非主干上下文内重复扫描与初始化系统字体库。

use super::layout::{BoxTextLayout, BoxedTextPaint};
use super::raster::{blend_glyph_rgba, GlyphPixel};
use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping, Style, SwashCache,
    Weight,
};
use std::cell::RefCell;
use tiny_skia::PixmapMut;

thread_local! {
    static THREAD_LOCAL_TEXT_RENDERER: RefCell<TextRenderer> = RefCell::new(TextRenderer::new());
}

/// 包围盒自适应排版的字号下限（低于该值中英文笔画开始糊连，不再继续压缩）
const MIN_BOX_FONT_SIZE: f32 = 9.0;
/// 包围盒自适应排版的字号上限
const MAX_BOX_FONT_SIZE: f32 = 48.0;
/// 溢出时单次字号收缩比例
const FONT_SHRINK_STEP: f32 = 0.85;
/// 字号收缩尝试次数上限，避免长文本触发无界排版循环
const FONT_SHRINK_ATTEMPTS: usize = 3;

/// 纯 Rust 高精度文本渲染上下文
pub struct TextRenderer {
    pub(super) font_system: FontSystem,
    pub(super) swash_cache: SwashCache,
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// 文本排版外接矩形与样式参数配置
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextBoxOptions {
    /// 矩形区域 (x, y, width, height)
    pub rect: (f32, f32, f32, f32),
    /// 建议字号 (pt)
    pub font_size: f32,
    /// 文本颜色 RGBA
    pub rgba: (u8, u8, u8, u8),
}

/// 单行文本附加排版样式。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextStyleOptions<'a> {
    pub bold: bool,
    pub italic: bool,
    pub font_family: Option<&'a str>,
}

/// 单行样式文字绘制参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyledTextDrawOptions<'a> {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub rgba: (u8, u8, u8, u8),
    pub style: TextStyleOptions<'a>,
}

/// 单行样式文本排版所需的文本、画布与样式。
#[derive(Clone, Copy)]
struct StyledShapeOptions<'a> {
    text: &'a str,
    width: f32,
    height: f32,
    font_size: f32,
    style: TextStyleOptions<'a>,
}

/// 将已排版缓冲区混合到画布所需的参数。
struct StyledBufferPaint<'a, 'p> {
    pixmap: &'a mut PixmapMut<'p>,
    buffer: &'a mut Buffer,
    origin_x: f32,
    origin_y: f32,
    rgba: (u8, u8, u8, u8),
}

/// 输入框文本测量的字号、行高与可用画布范围。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMeasureOptions {
    /// 字号
    pub font_size: f32,
    /// 行高
    pub line_height: f32,
    /// 最大可用宽度
    pub max_width: f32,
    /// 最大可用高度
    pub max_height: f32,
}

impl TextMeasureOptions {
    /// 构造文本测量配置。
    pub const fn new(font_size: f32, line_height: f32, max_width: f32, max_height: f32) -> Self {
        Self {
            font_size,
            line_height,
            max_width,
            max_height,
        }
    }
}

/// 单行纯文本绘制的位置、字号与颜色参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextDrawOptions {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub rgba: (u8, u8, u8, u8),
}

impl TextDrawOptions {
    /// 构造单行文本绘制参数。
    pub const fn new(x: f32, y: f32, font_size: f32, rgba: (u8, u8, u8, u8)) -> Self {
        Self {
            x,
            y,
            font_size,
            rgba,
        }
    }
}

/// 将文本样式转换为 cosmic-text 属性。
fn styled_attrs(style: TextStyleOptions<'_>) -> Attrs<'_> {
    let mut attrs = Attrs::new();
    if let Some(family) = style.font_family {
        attrs = attrs.family(Family::Name(family));
    }
    attrs
        .weight(if style.bold {
            Weight::BOLD
        } else {
            Weight::NORMAL
        })
        .style(if style.italic {
            Style::Italic
        } else {
            Style::Normal
        })
}

impl TextRenderer {
    /// 构造新的文本渲染器并初始化字体缓存
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        }
    }

    /// 在指定坐标绘制中文或英文排版文字（使用 RGBA 原始字节四元组）
    pub fn draw_text_rgba(&mut self, pixmap: &mut PixmapMut, text: &str, options: TextDrawOptions) {
        self.draw_text_styled_rgba(
            pixmap,
            text,
            StyledTextDrawOptions {
                x: options.x,
                y: options.y,
                font_size: options.font_size,
                rgba: options.rgba,
                style: TextStyleOptions::default(),
            },
        );
    }

    /// 在指定坐标绘制多行自动折行文本。
    pub fn draw_multiline_text_rgba(
        &mut self,
        pixmap: &mut PixmapMut,
        text: &str,
        origin_x: f32,
        origin_y: f32,
        options: TextMeasureOptions,
        rgba: (u8, u8, u8, u8),
    ) {
        if text.is_empty() {
            return;
        }
        let metrics = Metrics::new(options.font_size, options.line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, options.max_width, options.max_height);
        buffer.set_text(&mut self.font_system, text, Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);
        self.paint_styled_buffer(StyledBufferPaint {
            pixmap,
            buffer: &mut buffer,
            origin_x,
            origin_y,
            rgba,
        });
    }

    /// 在指定坐标绘制带粗体、斜体和字体族样式的单行文字。
    pub fn draw_text_styled_rgba(
        &mut self,
        pixmap: &mut PixmapMut,
        text: &str,
        options: StyledTextDrawOptions<'_>,
    ) {
        if text.is_empty() {
            return;
        }
        let mut buffer = self.shape_styled_text(StyledShapeOptions {
            text,
            width: pixmap.width() as f32,
            height: pixmap.height() as f32,
            font_size: options.font_size,
            style: options.style,
        });
        self.paint_styled_buffer(StyledBufferPaint {
            pixmap,
            buffer: &mut buffer,
            origin_x: options.x,
            origin_y: options.y,
            rgba: options.rgba,
        });
    }

    /// 按文本样式完成单行排版。
    fn shape_styled_text(&mut self, options: StyledShapeOptions<'_>) -> Buffer {
        let metrics = Metrics::new(options.font_size, options.font_size * 1.25);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, options.width, options.height);
        let attrs = styled_attrs(options.style);
        buffer.set_text(
            &mut self.font_system,
            options.text,
            attrs,
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }

    /// 将排版缓冲区按画布边界混合到目标像素。
    fn paint_styled_buffer(&mut self, paint: StyledBufferPaint<'_, '_>) {
        let StyledBufferPaint {
            pixmap,
            buffer,
            origin_x,
            origin_y,
            rgba,
        } = paint;
        let width = pixmap.width() as i32;
        let height = pixmap.height() as i32;
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            CosmicColor::rgba(rgba.0, rgba.1, rgba.2, rgba.3),
            |gx, gy, _w, _h, color| {
                let px = origin_x as i32 + gx;
                let py = origin_y as i32 + gy;
                if px >= 0 && px < width && py >= 0 && py < height {
                    let index = ((py as usize) * (width as usize) + (px as usize)) * 4;
                    blend_glyph_rgba(
                        pixmap.data_mut(),
                        index,
                        GlyphPixel {
                            red: color.r(),
                            green: color.g(),
                            blue: color.b(),
                            alpha: color.a(),
                        },
                    );
                }
            },
        );
    }

    /// 在指定矩形包围盒内自适应换行排版并绘制文字（支持自动折行与字号动态调整）
    ///
    /// 文本高度溢出包围盒时按 `FONT_SHRINK_STEP` 逐次压缩字号，最多尝试
    /// `FONT_SHRINK_ATTEMPTS` 次；无论最终是否收敛，落笔一律裁剪在盒形区域内。
    pub fn draw_text_box_rgba(
        &mut self,
        pixmap: &mut PixmapMut,
        text: &str,
        options: TextBoxOptions,
    ) {
        let (_x, _y, box_w, box_h) = options.rect;
        if text.trim().is_empty() || box_w <= 1.0 || box_h <= 1.0 {
            return;
        }

        let mut font_size = options
            .font_size
            .clamp(MIN_BOX_FONT_SIZE, MAX_BOX_FONT_SIZE);

        for attempt in 0..FONT_SHRINK_ATTEMPTS {
            let line_height = font_size * 1.25;
            let mut buffer = self.shape_box_text(
                text,
                BoxTextLayout {
                    font_size,
                    line_height,
                    box_width: box_w,
                    box_height: box_h,
                },
            );
            let total_text_h = (buffer.layout_runs().count() as f32) * line_height;

            // 收手条件：高度已落入包围盒、字号触底、或收缩次数耗尽。
            // 末次即使文本仍然溢出也直接落笔，由盒形裁剪兜住越界字形，
            // 杜绝旧实现回退到整幅画布排版导致文字扩散覆盖到周边标注与原图的问题。
            let is_last_attempt = attempt + 1 == FONT_SHRINK_ATTEMPTS;
            if total_text_h <= box_h || font_size <= MIN_BOX_FONT_SIZE || is_last_attempt {
                let vertical_offset = ((box_h - total_text_h) / 2.0).max(0.0);
                self.paint_buffer_in_box(
                    pixmap,
                    BoxedTextPaint {
                        buffer: &mut buffer,
                        options: &options,
                        vertical_offset,
                    },
                );
                return;
            }

            font_size = (font_size * FONT_SHRINK_STEP).max(MIN_BOX_FONT_SIZE);
        }
    }
}

/// 借用线程局部文本渲染器执行闭包，避免在临时标注图元中反复分配字体引擎
pub fn with_thread_local_text_renderer<R>(f: impl FnOnce(&mut TextRenderer) -> R) -> R {
    THREAD_LOCAL_TEXT_RENDERER.with(|cell| {
        let mut renderer = cell.borrow_mut();
        f(&mut renderer)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    #[test]
    fn test_text_renderer_basic_draw() {
        let mut pixmap = Pixmap::new(100, 40).unwrap();
        let mut renderer = TextRenderer::new();
        renderer.draw_text_rgba(
            &mut pixmap.as_mut(),
            "Snapin",
            TextDrawOptions {
                x: 10.0,
                y: 10.0,
                font_size: 16.0,
                rgba: (255, 255, 255, 255),
            },
        );
        let has_pixel = pixmap.data().iter().any(|&b| b > 0);
        assert!(has_pixel, "排版绘制应写入非零像素");
    }

    #[test]
    fn test_thread_local_text_renderer() {
        let mut pixmap = Pixmap::new(50, 50).unwrap();
        with_thread_local_text_renderer(|renderer| {
            renderer.draw_centered_bold_text(
                &mut pixmap.as_mut(),
                "1",
                crate::layout::CenteredTextOptions {
                    center: (25.0, 25.0),
                    font_size: 15.0,
                    max_extent: 60.0,
                    rgba: (255, 255, 255, 255),
                },
            );
        });
        let has_pixel = pixmap.data().iter().any(|&b| b > 0);
        assert!(has_pixel, "标号数字排版绘制应写入非零像素");
    }

    #[test]
    fn test_draw_text_box_rgba_wrapping() {
        let mut pixmap = Pixmap::new(120, 80).unwrap();
        let mut renderer = TextRenderer::new();
        renderer.draw_text_box_rgba(
            &mut pixmap.as_mut(),
            "This is a longer translation text that wraps",
            TextBoxOptions {
                rect: (5.0, 5.0, 100.0, 60.0),
                font_size: 14.0,
                rgba: (0, 0, 0, 255),
            },
        );
        let has_pixel = pixmap.data().iter().any(|&b| b > 0);
        assert!(has_pixel, "换行排版绘制应写入非零像素");
    }

    /// 断言字号收缩耗尽后的超长文本仍被严格裁剪在包围盒内，不外溢污染周边画布
    #[test]
    fn test_draw_text_box_rgba_clips_overflow_into_box() {
        let rect = (40.0f32, 30.0f32, 60.0f32, 24.0f32);
        let mut pixmap = Pixmap::new(200, 120).unwrap();
        let mut renderer = TextRenderer::new();
        renderer.draw_text_box_rgba(
            &mut pixmap.as_mut(),
            &"overflow translation text".repeat(12),
            TextBoxOptions {
                rect,
                font_size: 48.0,
                rgba: (255, 255, 255, 255),
            },
        );

        let (bx, by, bw, bh) = rect;
        let (min_x, min_y) = (bx as usize, by as usize);
        let (max_x, max_y) = ((bx + bw).floor() as usize, (by + bh).floor() as usize);
        let stride = pixmap.width() as usize;

        let mut painted = 0usize;
        for (idx, pixel) in pixmap.data().as_chunks::<4>().0.iter().enumerate() {
            if pixel.iter().all(|&byte| byte == 0) {
                continue;
            }
            painted += 1;
            let px = idx % stride;
            let py = idx / stride;
            assert!(
                px >= min_x && px < max_x && py >= min_y && py < max_y,
                "溢出文字越出包围盒，坐标: ({px}, {py})"
            );
        }
        assert!(painted > 0, "裁剪后仍应写入可见字形像素");
    }
}
