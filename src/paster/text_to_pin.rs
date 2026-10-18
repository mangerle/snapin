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

use crate::paster::context_menu::render::TextRenderer;

/// 将纯文本行转换为带卡片背景的便签 Pixmap
pub fn render_simple_text_card(lines: &[&str]) -> Option<Pixmap> {
    if lines.is_empty() {
        return None;
    }

    let padding_x = 20.0f32;
    let padding_y = 16.0f32;
    let line_height = 24.0f32;
    let font_size = 14.0f32;

    // 估算内容包围盒
    let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1);
    let card_w = (max_line_len as f32 * 10.0 + padding_x * 2.0).clamp(260.0, 960.0);
    let card_h = (lines.len() as f32 * line_height + padding_y * 2.0 + 8.0).max(64.0);

    let mut pixmap = Pixmap::new(card_w as u32, card_h as u32)?;

    // 1. 绘制暗黑现代便签卡片主体
    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(30, 32, 42, 252);
    let card_rect = Rect::from_xywh(0.0, 0.0, card_w, card_h)?;
    pixmap.fill_rect(card_rect, &bg_paint, Transform::identity(), None);

    // 2. 绘制顶部便签精致天蓝指示细线条
    let mut bar_paint = Paint::default();
    bar_paint.set_color_rgba8(59, 130, 246, 255);
    if let Some(bar_rect) = Rect::from_xywh(0.0, 0.0, card_w, 3.5) {
        pixmap.fill_rect(bar_rect, &bar_paint, Transform::identity(), None);
    }

    // 3. 绘制卡片微弱半透明描边
    let mut border_paint = Paint::default();
    border_paint.set_color_rgba8(255, 255, 255, 28);
    let stroke = Stroke {
        width: 1.0,
        ..Default::default()
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(card_rect);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);
    }

    // 4. 逐行真实排版渲染文本内容
    let mut text_renderer = TextRenderer::default();
    let text_color = tiny_skia::Color::from_rgba8(240, 242, 248, 255);
    let mut cur_y = padding_y + 4.0;

    for line in lines {
        if !line.is_empty() {
            text_renderer.draw_text(
                &mut pixmap.as_mut(),
                line,
                padding_x,
                cur_y,
                font_size,
                text_color,
            );
        }
        cur_y += line_height;
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

    #[test]
    fn test_render_simple_text_card() {
        let lines = vec!["fn main() {", "    println!(\"Hello Snapin\");", "}"];
        let card = render_simple_text_card(&lines);
        assert!(card.is_some());
        let pixmap = card.unwrap();
        assert!(pixmap.width() >= 260);
        assert!(pixmap.height() >= 64);
    }
}
