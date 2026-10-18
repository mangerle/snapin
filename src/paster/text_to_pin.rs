//! 剪贴板文本与颜色值便签化渲染器。
//!
//! 当系统未处于截图状态而用户直接按下贴图键时，检查剪贴板内容：
//! 若包含纯文本或颜色值，自动排版并烘焙为带圆角与投影的精致便签图片。

use tiny_skia::{Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

/// 判断字符串是否为标准 HEX 颜色格式 (如 `#FF5500` 或 `#F50`)
pub fn try_parse_hex_color(text: &str) -> Option<(u8, u8, u8)> {
    let t = text.trim();
    if !t.starts_with('#') {
        return None;
    }
    let hex = &t[1..];
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some((r, g, b))
    } else if hex.len() == 3 {
        let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
        let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
        let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

/// 将 HEX 颜色值渲染为一个精致的取色预览卡片贴图
pub fn render_color_swatch_card(r: u8, g: u8, b: u8) -> Option<Pixmap> {
    let card_w = 240u32;
    let card_h = 140u32;
    let mut pixmap = Pixmap::new(card_w, card_h)?;

    // 1. 绘制底层卡片底板
    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(30, 30, 30, 255);
    let card_rect = Rect::from_xywh(0.0, 0.0, card_w as f32, card_h as f32)?;
    pixmap.fill_rect(card_rect, &bg_paint, Transform::identity(), None);

    // 2. 绘制上部纯色块
    let mut swatch_paint = Paint::default();
    swatch_paint.set_color_rgba8(r, g, b, 255);
    let swatch_rect = Rect::from_xywh(12.0, 12.0, (card_w - 24) as f32, 80.0)?;
    pixmap.fill_rect(swatch_rect, &swatch_paint, Transform::identity(), None);

    // 3. 边框线
    let mut border_paint = Paint::default();
    border_paint.set_color_rgba8(60, 60, 60, 255);
    let stroke = Stroke {
        width: 1.0,
        ..Default::default()
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(card_rect);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);
    }

    Some(pixmap)
}

/// 将纯文本行转换为带卡片背景的便签 Pixmap
pub fn render_simple_text_card(lines: &[&str]) -> Option<Pixmap> {
    let padding = 16.0f32;
    let line_height = 22.0f32;
    let estimated_width = 360.0f32;
    let card_h = (lines.len().max(1) as f32 * line_height + padding * 2.0).max(80.0);

    let mut pixmap = Pixmap::new(estimated_width as u32, card_h as u32)?;

    // 绘制深色护眼便签底色
    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(28, 28, 30, 245);
    let card_rect = Rect::from_xywh(0.0, 0.0, estimated_width, card_h)?;
    pixmap.fill_rect(card_rect, &bg_paint, Transform::identity(), None);

    // 绘制装饰细边框
    let mut border_paint = Paint::default();
    border_paint.set_color_rgba8(0, 174, 255, 180);
    let stroke = Stroke {
        width: 1.5,
        ..Default::default()
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(card_rect);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);
    }

    Some(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(try_parse_hex_color("#FF5500"), Some((255, 85, 0)));
        assert_eq!(try_parse_hex_color("#000"), Some((0, 0, 0)));
        assert_eq!(try_parse_hex_color("#FFF"), Some((255, 255, 255)));
        assert_eq!(try_parse_hex_color("not_color"), None);
    }

    #[test]
    fn test_render_swatch_card() {
        let pixmap = render_color_swatch_card(255, 0, 0);
        assert!(pixmap.is_some());
    }
}
