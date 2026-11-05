//! 截图美化导出渲染。
//!
//! 输入已经合成全部标注的截图位图，输出带背景、圆角、阴影、社交画布和水印的
//! 新位图。渲染完全基于 tiny-skia，不经过平台窗口，因此可在后台线程安全执行。

use crate::{with_thread_local_text_renderer, RenderError, TextDrawOptions};
use snapin_foundation::beautify::{BeautifyBackground, BeautifyOptions};
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Mask, MaskType, Paint, PathBuilder, Pixmap,
    PixmapPaint, PixmapRef, Point, SpreadMode, Transform,
};

/// 渲染美化截图。
///
/// # Errors
///
/// 输入尺寸为零、输出画布尺寸非法、像素缓冲区分配失败或背景渐变构造失败时
/// 返回 [`RenderError`]。
pub fn render_beautified(
    input: PixmapRef<'_>,
    options: &BeautifyOptions,
) -> Result<Pixmap, RenderError> {
    if input.width() == 0 || input.height() == 0 {
        return Err(RenderError::EmptyInput);
    }
    let layout = BeautifyLayout::calculate(input, options)?;
    let mut output = Pixmap::new(layout.canvas_width, layout.canvas_height).ok_or(
        RenderError::CanvasAllocation {
            width: layout.canvas_width,
            height: layout.canvas_height,
        },
    )?;
    draw_background(&mut output, options.background)?;
    draw_beautified_content(&mut output, input, options, &layout)?;
    draw_watermark(&mut output, options, &layout);
    Ok(output)
}

/// 输出画布内贴图内容的位置与尺寸。
#[derive(Debug, Clone, Copy)]
struct ContentRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// 美化输出所需的画布、内容与缩放参数。
#[derive(Debug, Clone, Copy)]
struct BeautifyLayout {
    canvas_width: u32,
    canvas_height: u32,
    content: ContentRect,
    scale: f32,
    corner_radius: f32,
}

impl BeautifyLayout {
    /// 校验输出上限并计算居中贴图区域。
    fn calculate(input: PixmapRef<'_>, options: &BeautifyOptions) -> Result<Self, RenderError> {
        let (canvas_width, canvas_height) =
            options.social_size.canvas_size().unwrap_or_else(|| {
                let width = input
                    .width()
                    .saturating_add((options.padding * 2.0).round() as u32);
                let height = input
                    .height()
                    .saturating_add((options.padding * 2.0).round() as u32);
                (width, height)
            });
        if canvas_width == 0 || canvas_height == 0 || canvas_width > 8192 || canvas_height > 8192 {
            return Err(RenderError::InvalidCanvasSize {
                width: canvas_width,
                height: canvas_height,
            });
        }

        let available_width = (canvas_width as f32 - options.padding * 2.0).max(1.0);
        let available_height = (canvas_height as f32 - options.padding * 2.0).max(1.0);
        let scale = (available_width / input.width() as f32)
            .min(available_height / input.height() as f32)
            .min(1.0);
        let content_width = (input.width() as f32 * scale).round().max(1.0);
        let content_height = (input.height() as f32 * scale).round().max(1.0);
        Ok(Self {
            canvas_width,
            canvas_height,
            content: ContentRect {
                x: (canvas_width as f32 - content_width) / 2.0,
                y: (canvas_height as f32 - content_height) / 2.0,
                width: content_width,
                height: content_height,
            },
            scale,
            corner_radius: options.corner_radius * scale.max(0.5),
        })
    }
}

/// 依次绘制阴影、缩放图像与圆角遮罩。
fn draw_beautified_content(
    output: &mut Pixmap,
    input: PixmapRef<'_>,
    options: &BeautifyOptions,
    layout: &BeautifyLayout,
) -> Result<(), RenderError> {
    if options.shadow {
        draw_shadow(output, layout.content, layout.corner_radius);
    }
    let scaled = scale_content(input, layout)?;
    let mask = build_content_mask(layout)?;
    output.draw_pixmap(
        layout.content.x as i32,
        layout.content.y as i32,
        scaled.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        Some(&mask),
    );
    Ok(())
}

/// 将原始图像缩放到美化内容区域。
fn scale_content(input: PixmapRef<'_>, layout: &BeautifyLayout) -> Result<Pixmap, RenderError> {
    let width = layout.content.width as u32;
    let height = layout.content.height as u32;
    let mut scaled =
        Pixmap::new(width, height).ok_or(RenderError::ScaledAllocation { width, height })?;
    scaled.draw_pixmap(
        0,
        0,
        input,
        &PixmapPaint::default(),
        Transform::from_scale(layout.scale, layout.scale),
        None,
    );
    Ok(scaled)
}

/// 构造内容区域圆角遮罩。
fn build_content_mask(layout: &BeautifyLayout) -> Result<Mask, RenderError> {
    let width = layout.content.width as u32;
    let height = layout.content.height as u32;
    let mut mask_pixmap =
        Pixmap::new(width, height).ok_or(RenderError::MaskAllocation { width, height })?;
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: layout.content.x,
            y: layout.content.y,
            width: layout.content.width,
            height: layout.content.height,
            radius: layout.corner_radius,
        },
    );
    let Some(path) = builder.finish() else {
        return Err(RenderError::MaskPath {
            width: layout.content.width,
            height: layout.content.height,
            radius: layout.corner_radius,
        });
    };
    let mut paint = Paint::default();
    paint.set_color(Color::WHITE);
    mask_pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::from_translate(-layout.content.x, -layout.content.y),
        None,
    );
    Ok(Mask::from_pixmap(mask_pixmap.as_ref(), MaskType::Alpha))
}

/// 在画布右下角绘制可选水印。
fn draw_watermark(output: &mut Pixmap, options: &BeautifyOptions, layout: &BeautifyLayout) {
    let Some(watermark) = options.watermark.as_deref() else {
        return;
    };
    if watermark.trim().is_empty() {
        return;
    }
    with_thread_local_text_renderer(|renderer| {
        let font_size = (layout.canvas_width as f32 * 0.018).clamp(12.0, 28.0);
        renderer.draw_text_rgba(
            &mut output.as_mut(),
            watermark,
            TextDrawOptions {
                x: layout.canvas_width as f32 - font_size * watermark.chars().count() as f32 - 24.0,
                y: layout.canvas_height as f32 - font_size - 22.0,
                font_size,
                rgba: (255, 255, 255, 185),
            },
        );
    });
}

/// 绘制纯色或渐变背景。
fn draw_background(output: &mut Pixmap, background: BeautifyBackground) -> Result<(), RenderError> {
    let colors = match background {
        BeautifyBackground::Dark => (Color::from_rgba8(24, 29, 39, 255), None),
        BeautifyBackground::Indigo => (
            Color::from_rgba8(30, 64, 175, 255),
            Some(Color::from_rgba8(147, 51, 234, 255)),
        ),
        BeautifyBackground::Sunset => (
            Color::from_rgba8(249, 115, 22, 255),
            Some(Color::from_rgba8(225, 29, 72, 255)),
        ),
        BeautifyBackground::Mint => (
            Color::from_rgba8(13, 148, 136, 255),
            Some(Color::from_rgba8(34, 197, 94, 255)),
        ),
    };
    let mut paint = Paint::default();
    if let Some(end_color) = colors.1 {
        let shader = LinearGradient::new(
            Point::from_xy(0.0, 0.0),
            Point::from_xy(output.width() as f32, output.height() as f32),
            vec![
                GradientStop::new(0.0, colors.0),
                GradientStop::new(1.0, end_color),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .ok_or(RenderError::BackgroundGradient)?;
        paint.shader = shader;
    } else {
        paint.set_color(colors.0);
    }
    let Some(rect) =
        tiny_skia::Rect::from_xywh(0.0, 0.0, output.width() as f32, output.height() as f32)
    else {
        return Err(RenderError::BackgroundRect {
            width: output.width(),
            height: output.height(),
        });
    };
    output.fill_rect(rect, &paint, Transform::identity(), None);
    Ok(())
}

/// 使用多层半透明圆角矩形近似柔和阴影。
fn draw_shadow(output: &mut Pixmap, content: ContentRect, radius: f32) {
    for step in (1..=6).rev() {
        let spread = step as f32 * 4.0;
        let alpha = (4 + (7 - step) * 4) as u8;
        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 0, alpha);
        let mut path = PathBuilder::new();
        add_round_rect(
            &mut path,
            RoundRect {
                x: content.x - spread,
                y: content.y - spread + 8.0,
                width: content.width + spread * 2.0,
                height: content.height + spread * 2.0,
                radius: radius + spread,
            },
        );
        if let Some(path) = path.finish() {
            output.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

/// 圆角矩形几何参数。
#[derive(Debug, Clone, Copy)]
struct RoundRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
}

/// 向路径追加圆角矩形。
fn add_round_rect(path: &mut PathBuilder, rect: RoundRect) {
    let r = rect.radius.max(0.0).min(rect.width.min(rect.height) / 2.0);
    path.move_to(rect.x + r, rect.y);
    path.line_to(rect.x + rect.width - r, rect.y);
    path.quad_to(rect.x + rect.width, rect.y, rect.x + rect.width, rect.y + r);
    path.line_to(rect.x + rect.width, rect.y + rect.height - r);
    path.quad_to(
        rect.x + rect.width,
        rect.y + rect.height,
        rect.x + rect.width - r,
        rect.y + rect.height,
    );
    path.line_to(rect.x + r, rect.y + rect.height);
    path.quad_to(
        rect.x,
        rect.y + rect.height,
        rect.x,
        rect.y + rect.height - r,
    );
    path.line_to(rect.x, rect.y + r);
    path.quad_to(rect.x, rect.y, rect.x + r, rect.y);
    path.close();
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapin_foundation::beautify::SocialSize;

    /// 原始尺寸输出必须包含四周留白且角点保持背景色。
    #[test]
    fn test_render_beautified_original_size_has_transparent_rounded_corners() {
        let mut input = Pixmap::new(40, 20).expect("测试输入位图创建成功");
        input.fill(Color::from_rgba8(255, 0, 0, 255));
        let output =
            render_beautified(input.as_ref(), &BeautifyOptions::default()).expect("美化渲染应成功");
        assert_eq!(output.width(), 136);
        assert_eq!(output.height(), 116);
        let corner = output.pixel(0, 0).expect("左上角像素应存在");
        assert!(corner.alpha() > 0, "背景应覆盖整个输出画布");
    }

    /// 社交尺寸必须使用固定画布。
    #[test]
    fn test_render_beautified_social_size() {
        let mut input = Pixmap::new(300, 200).expect("测试输入位图创建成功");
        input.fill(Color::from_rgba8(0, 0, 255, 255));
        let options = BeautifyOptions {
            social_size: SocialSize::Square,
            ..BeautifyOptions::default()
        };
        let output = render_beautified(input.as_ref(), &options).expect("社交画布渲染应成功");
        assert_eq!((output.width(), output.height()), (1080, 1080));
    }
}
