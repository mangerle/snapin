//! 放大镜取色器与极速取色闭环实现。
//!
//! 支持像素级局部放大、参考网格线显示以及按 C 键直接复制 HEX/RGB 颜色。

use image::RgbaImage;
use tiny_skia::{Paint, PathBuilder, PixmapMut, Rect as SkiaRect, Stroke, Transform};

pub use snapin_core::config::ColorFormat;

/// 放大镜控制器
pub struct Magnifier {
    pub enabled: bool,
    pub scale: u32,
    pub format: ColorFormat,
    pub show_grid: bool,
    pub ui_scale: f32,
}

impl Default for Magnifier {
    fn default() -> Self {
        Self {
            enabled: true,
            scale: 8,
            format: ColorFormat::Hex,
            show_grid: true,
            ui_scale: 1.0,
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

    /// 在给定的目标 pixmap 上绘制跟随光标的放大镜卡片 (自适应 HiDPI 缩放)
    pub fn render(&self, pixmap: &mut PixmapMut, img: &RgbaImage, cursor_x: i32, cursor_y: i32) {
        if !self.enabled {
            return;
        }

        let (_cur_r, _cur_g, _cur_b, _) = self.get_pixel_color(img, cursor_x, cursor_y);

        let s = self.ui_scale;
        // 放大镜尺寸与采样网格 (采样 15x15 像素，按设置放大倍率呈现)
        let sample_radius = 7i32;
        let cell_size = (self.scale as f32 * s).round().max(4.0);
        let box_w = (sample_radius * 2 + 1) as f32 * cell_size;
        let box_h = box_w;

        // 动态偏移：防止放大镜挡住光标本身或超出屏幕边界
        let offset = 20.0 * s;
        let mut target_x = cursor_x as f32 + offset;
        let mut target_y = cursor_y as f32 + offset;
        if target_x + box_w > pixmap.width() as f32 {
            target_x = (cursor_x as f32 - box_w - offset).max(0.0);
        }
        if target_y + box_h + 30.0 * s > pixmap.height() as f32 {
            target_y = (cursor_y as f32 - box_h - (offset + 20.0 * s)).max(0.0);
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

                    if self.show_grid {
                        let mut grid_paint = Paint::default();
                        grid_paint.set_color_rgba8(0, 0, 0, 35);
                        let stroke = Stroke {
                            width: 0.5,
                            ..Default::default()
                        };
                        let mut pb = PathBuilder::new();
                        pb.push_rect(rect);
                        if let Some(path) = pb.finish() {
                            pixmap.stroke_path(
                                &path,
                                &grid_paint,
                                &stroke,
                                Transform::identity(),
                                None,
                            );
                        }
                    }
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
                width: 2.0 * s,
                ..Default::default()
            };
            let mut pb = PathBuilder::new();
            pb.push_rect(rect);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
            }
        }

        // 3. 绘制外边框与底部色值展示卡片背景
        let bottom_bar_h = 24.0 * s;
        if let Some(border_rect) =
            SkiaRect::from_xywh(target_x, target_y, box_w, box_h + bottom_bar_h)
        {
            let mut border_paint = Paint::default();
            border_paint.set_color_rgba8(20, 20, 20, 220);
            // 底部颜色文本卡槽
            if let Some(text_bg) =
                SkiaRect::from_xywh(target_x, target_y + box_h, box_w, bottom_bar_h)
            {
                pixmap.fill_rect(text_bg, &border_paint, Transform::identity(), None);
            }

            let stroke = Stroke {
                width: 2.0 * s,
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
