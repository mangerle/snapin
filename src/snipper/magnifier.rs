//! 放大镜取色器与极速取色闭环实现。
//!
//! 支持像素级局部放大、参考网格线显示以及按 C 键直接复制 HEX/RGB 颜色。

use image::RgbaImage;
use tiny_skia::{Paint, PathBuilder, PixmapMut, Rect as SkiaRect, Stroke, Transform};

/// 取色格式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorFormat {
    #[default]
    Hex,
    Rgb,
    Hsl,
}

impl ColorFormat {
    /// 轮转切换至下一种取色格式 (Shift+C 触发)
    pub fn next(&self) -> Self {
        match self {
            Self::Hex => Self::Rgb,
            Self::Rgb => Self::Hsl,
            Self::Hsl => Self::Hex,
        }
    }

    /// 格式化 RGBA 颜色为可读文本
    pub fn format_color(&self, r: u8, g: u8, b: u8) -> String {
        match self {
            Self::Hex => format!("#{r:02X}{g:02X}{b:02X}"),
            Self::Rgb => format!("rgb({r}, {g}, {b})"),
            Self::Hsl => {
                let (h, s, l) = rgb_to_hsl(r, g, b);
                format!("hsl({h}, {s}%, {l}%)")
            }
        }
    }
}

/// 放大镜控制器
pub struct Magnifier {
    pub scale: u32,
    pub format: ColorFormat,
    pub show_grid: bool,
}

impl Default for Magnifier {
    fn default() -> Self {
        Self {
            scale: 8,
            format: ColorFormat::Hex,
            show_grid: true,
        }
    }
}

impl Magnifier {
    /// 读取背景图像指定坐标的像素值 (r, g, b, a)
    pub fn get_pixel_color(&self, img: &RgbaImage, x: i32, y: i32) -> (u8, u8, u8, u8) {
        if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
            (0, 0, 0, 255)
        } else {
            let p = img.get_pixel(x as u32, y as u32);
            (p[0], p[1], p[2], p[3])
        }
    }

    /// 在给定的目标 pixmap 上绘制跟随光标的放大镜卡片
    pub fn render(&self, pixmap: &mut PixmapMut, img: &RgbaImage, cursor_x: i32, cursor_y: i32) {
        let (_cur_r, _cur_g, _cur_b, _) = self.get_pixel_color(img, cursor_x, cursor_y);

        // 放大镜尺寸与采样网格 (采样 15x15 像素，放大为 150x150)
        let sample_radius = 7i32;
        let cell_size = 10.0f32;
        let box_w = (sample_radius * 2 + 1) as f32 * cell_size;
        let box_h = box_w;

        // 动态偏移：防止放大镜挡住光标本身或超出屏幕边界
        let mut target_x = cursor_x as f32 + 20.0;
        let mut target_y = cursor_y as f32 + 20.0;
        if target_x + box_w > pixmap.width() as f32 {
            target_x = (cursor_x as f32 - box_w - 20.0).max(0.0);
        }
        if target_y + box_h + 30.0 > pixmap.height() as f32 {
            target_y = (cursor_y as f32 - box_h - 40.0).max(0.0);
        }

        // 1. 逐个像素色块绘制
        for dy in -sample_radius..=sample_radius {
            for dx in -sample_radius..=sample_radius {
                let px = cursor_x + dx;
                let py = cursor_y + dy;
                let (r, g, b, _) = self.get_pixel_color(img, px, py);

                let cell_x = target_x + (dx + sample_radius) as f32 * cell_size;
                let cell_y = target_y + (dy + sample_radius) as f32 * cell_size;

                if let Some(rect) = SkiaRect::from_xywh(cell_x, cell_y, cell_size, cell_size) {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(r, g, b, 255);
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                }
            }
        }

        // 2. 绘制中心十字瞄准准星
        let center_x = target_x + sample_radius as f32 * cell_size;
        let center_y = target_y + sample_radius as f32 * cell_size;
        if let Some(rect) = SkiaRect::from_xywh(center_x, center_y, cell_size, cell_size) {
            let mut stroke_paint = Paint::default();
            stroke_paint.set_color_rgba8(0, 174, 255, 255);
            let stroke = Stroke {
                width: 2.0,
                ..Default::default()
            };
            let mut pb = PathBuilder::new();
            pb.push_rect(rect);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
            }
        }

        // 3. 绘制外边框与底部色值展示卡片背景
        if let Some(border_rect) = SkiaRect::from_xywh(target_x, target_y, box_w, box_h + 24.0) {
            let mut border_paint = Paint::default();
            border_paint.set_color_rgba8(20, 20, 20, 220);
            // 底部颜色文本卡槽
            if let Some(text_bg) = SkiaRect::from_xywh(target_x, target_y + box_h, box_w, 24.0) {
                pixmap.fill_rect(text_bg, &border_paint, Transform::identity(), None);
            }

            let stroke = Stroke {
                width: 2.0,
                ..Default::default()
            };
            let mut outline_paint = Paint::default();
            outline_paint.set_color_rgba8(80, 80, 80, 255);
            let mut pb = PathBuilder::new();
            pb.push_rect(border_rect);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &outline_paint, &stroke, Transform::identity(), None);
            }
        }
    }
}

/// 辅助 RGB 转换为 HSL 算法
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (u16, u8, u8) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let l = (max + min) / 2.0;

    let (h, s) = if delta == 0.0 {
        (0.0, 0.0)
    } else {
        let s = if l < 0.5 {
            delta / (max + min)
        } else {
            delta / (2.0 - max - min)
        };

        let h = if max == rf {
            ((gf - bf) / delta) % 6.0
        } else if max == gf {
            ((bf - rf) / delta) + 2.0
        } else {
            ((rf - gf) / delta) + 4.0
        } * 60.0;

        let h = if h < 0.0 { h + 360.0 } else { h };
        (h, s)
    };

    (
        h.round() as u16,
        (s * 100.0).round() as u8,
        (l * 100.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_format_rotation() {
        let fmt = ColorFormat::Hex;
        assert_eq!(fmt.format_color(255, 0, 0), "#FF0000");

        let fmt = fmt.next();
        assert_eq!(fmt, ColorFormat::Rgb);
        assert_eq!(fmt.format_color(255, 0, 0), "rgb(255, 0, 0)");

        let fmt = fmt.next();
        assert_eq!(fmt, ColorFormat::Hsl);
        assert_eq!(fmt.format_color(255, 0, 0), "hsl(0, 100%, 50%)");

        let fmt = fmt.next();
        assert_eq!(fmt, ColorFormat::Hex);
    }
}
