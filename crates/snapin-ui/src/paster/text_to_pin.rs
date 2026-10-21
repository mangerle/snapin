//! 剪贴板文本与颜色值便签化渲染器。
//!
//! 当系统未处于截图状态而用户直接按下贴图键时，检查剪贴板内容：
//! 若包含纯文本或颜色值，自动排版并烘焙为带圆角与投影的精致便签图片。

use snapin_core::config::HexColor;
use snapin_core::render::with_thread_local_text_renderer;
use tiny_skia::{Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

/// 判断字符串是否为标准 HEX 颜色格式 (如 `#FF5500` 或 `#F50`)
///
/// 统一复用核心库的 ASCII 安全解析器，避免 UI 层自行切片导致非 ASCII 输入崩溃。
pub fn try_parse_hex_color(text: &str) -> Option<(u8, u8, u8)> {
    let trimmed = text.trim();
    if !trimmed.starts_with('#') {
        return None;
    }
    HexColor::from_hex(trimmed).map(HexColor::to_rgb)
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
    if lines.is_empty() {
        return None;
    }

    let padding_x = 24.0f32;
    let padding_y = 20.0f32;
    let line_height = 28.0f32;
    let font_size = 17.0f32;

    // 根据中英文字符实际排版特征精准估算宽度包围盒
    let max_line_w = lines
        .iter()
        .map(|l| {
            l.chars()
                .map(|c| if c.is_ascii() { 10.0 } else { 18.0 })
                .sum::<f32>()
        })
        .fold(0.0f32, f32::max);
    let card_w = (max_line_w + padding_x * 2.0).clamp(200.0, 1200.0);
    let card_h = (lines.len() as f32 * line_height + padding_y * 2.0 + 8.0).max(68.0);

    let mut pixmap = Pixmap::new(card_w as u32, card_h as u32)?;

    // 1. 绘制浅色便签卡片主体，与设置中心、右键菜单的浅色系保持一致
    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(255, 255, 255, 252);
    let card_rect = Rect::from_xywh(0.0, 0.0, card_w, card_h)?;
    pixmap.fill_rect(card_rect, &bg_paint, Transform::identity(), None);

    // 2. 绘制顶部便签精致天蓝指示细线条
    let mut bar_paint = Paint::default();
    bar_paint.set_color_rgba8(59, 130, 246, 255);
    if let Some(bar_rect) = Rect::from_xywh(0.0, 0.0, card_w, 3.5) {
        pixmap.fill_rect(bar_rect, &bar_paint, Transform::identity(), None);
    }

    // 3. 绘制卡片描边（浅底上用深色细边，避免白卡与白色背景糊在一起）
    let mut border_paint = Paint::default();
    border_paint.set_color_rgba8(15, 23, 42, 28);
    let stroke = Stroke {
        width: 1.0,
        ..Default::default()
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(card_rect);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);
    }

    // 4. 逐行真实排版渲染文本内容 (统一复用线程局部排版渲染器)
    let text_color = tiny_skia::Color::from_rgba8(15, 23, 42, 255);
    with_thread_local_text_renderer(|text_renderer| {
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
    });

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
        assert_eq!(try_parse_hex_color("#FF550080"), Some((255, 85, 0)));
        assert_eq!(try_parse_hex_color("not_color"), None);
        // 非 ASCII 输入必须安全返回 None，不能触发字符串切片 panic
        assert_eq!(try_parse_hex_color("#红色"), None);
        assert_eq!(try_parse_hex_color("#💥💥"), None);
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
        assert!(pixmap.width() >= 200);
        assert!(pixmap.height() >= 64);
    }
}
