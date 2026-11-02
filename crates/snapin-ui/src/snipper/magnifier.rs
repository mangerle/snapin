//! 放大镜取色器与极速取色闭环实现。
//!
//! 支持像素级局部放大、参考网格线显示以及按 C 键直接复制 HEX/RGB 颜色。

use crate::snipper::selection::Rect;
use image::RgbaImage;
use tiny_skia::{Paint, PathBuilder, PixmapMut, Rect as SkiaRect, Stroke, Transform};

pub use snapin_config::config::ColorFormat;

/// 放大镜采样半径：采样 (2r+1) x (2r+1) 像素
const SAMPLE_RADIUS: i32 = 7;

/// 放大镜卡片布局（物理像素），渲染与脏区计算共用同一套几何结果。
struct MagnifierLayout {
    target_x: f32,
    target_y: f32,
    box_w: f32,
    box_h: f32,
    cell_size: f32,
    bottom_bar_h: f32,
    /// 卡片描边向外溢出的余量，供脏区恢复时完整覆盖抗锯齿边缘
    margin: f32,
}

/// 放大镜计算所需的鼠标位置与视口尺寸。
#[derive(Debug, Clone, Copy)]
pub struct MagnifierViewport {
    /// 鼠标横坐标
    pub cursor_x: i32,
    /// 鼠标纵坐标
    pub cursor_y: i32,
    /// 视口宽度
    pub width: u32,
    /// 视口高度
    pub height: u32,
}

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

    /// 计算放大镜卡片布局：采样网格尺寸、屏幕边界避让与底部色值条高度。
    ///
    /// 返回 `None` 表示放大镜被禁用，调用方无需绘制也无需计算脏区。
    fn layout(&self, viewport: MagnifierViewport) -> Option<MagnifierLayout> {
        if !self.enabled {
            return None;
        }

        let s = self.ui_scale;
        // 放大镜尺寸与采样网格 (采样 15x15 像素，按设置放大倍率呈现)
        let cell_size = (self.scale as f32 * s).round().max(4.0);
        let box_w = (SAMPLE_RADIUS * 2 + 1) as f32 * cell_size;
        let box_h = box_w;

        // 动态偏移：防止放大镜挡住光标本身或超出屏幕边界
        let offset = 20.0 * s;
        let mut target_x = viewport.cursor_x as f32 + offset;
        let mut target_y = viewport.cursor_y as f32 + offset;
        if target_x + box_w > viewport.width as f32 {
            target_x = (viewport.cursor_x as f32 - box_w - offset).max(0.0);
        }
        if target_y + box_h + 30.0 * s > viewport.height as f32 {
            target_y = (viewport.cursor_y as f32 - box_h - (offset + 20.0 * s)).max(0.0);
        }

        Some(MagnifierLayout {
            target_x,
            target_y,
            box_w,
            box_h,
            cell_size,
            bottom_bar_h: 24.0 * s,
            margin: 2.0 * s,
        })
    }

    /// 返回放大镜卡片占据的像素矩形（含描边余量）。
    ///
    /// 供增量重绘精确抹除上一帧的放大镜像素，避免每帧整屏恢复底图。
    pub fn card_bounds(&self, viewport: MagnifierViewport) -> Option<Rect> {
        let layout = self.layout(viewport)?;
        Some(Rect {
            x: layout.target_x - layout.margin,
            y: layout.target_y - layout.margin,
            width: layout.box_w + layout.margin * 2.0,
            height: layout.box_h + layout.bottom_bar_h + layout.margin * 2.0,
        })
    }

    /// 在给定的目标 pixmap 上绘制跟随光标的放大镜卡片 (自适应 HiDPI 缩放)
    pub fn render(&self, pixmap: &mut PixmapMut, img: &RgbaImage, viewport: MagnifierViewport) {
        let Some(layout) = self.layout(viewport) else {
            return;
        };
        MagnifierRenderContext {
            magnifier: self,
            pixmap,
            image: img,
            cursor: (viewport.cursor_x, viewport.cursor_y),
            layout: &layout,
        }
        .render();
    }
}

/// 一帧放大镜绘制所需的只读与可变上下文。
struct MagnifierRenderContext<'a, 'p> {
    magnifier: &'a Magnifier,
    pixmap: &'a mut PixmapMut<'p>,
    image: &'a RgbaImage,
    cursor: (i32, i32),
    layout: &'a MagnifierLayout,
}

impl MagnifierRenderContext<'_, '_> {
    fn render(mut self) {
        self.render_sample_grid();
        self.render_center_reticle();
        self.render_frame();
    }

    /// 绘制采样色块，并在启用时将网格合并为单条路径。
    fn render_sample_grid(&mut self) {
        let mut cell_paint = Paint::default();
        let mut grid_path = PathBuilder::new();
        for dy in -SAMPLE_RADIUS..=SAMPLE_RADIUS {
            for dx in -SAMPLE_RADIUS..=SAMPLE_RADIUS {
                let (r, g, b, _) = self.magnifier.get_pixel_color(
                    self.image,
                    self.cursor.0 + dx,
                    self.cursor.1 + dy,
                );
                let cell_x =
                    self.layout.target_x + (dx + SAMPLE_RADIUS) as f32 * self.layout.cell_size;
                let cell_y =
                    self.layout.target_y + (dy + SAMPLE_RADIUS) as f32 * self.layout.cell_size;
                if let Some(rect) = SkiaRect::from_xywh(
                    cell_x,
                    cell_y,
                    self.layout.cell_size,
                    self.layout.cell_size,
                ) {
                    cell_paint.set_color_rgba8(r, g, b, 255);
                    self.pixmap
                        .fill_rect(rect, &cell_paint, Transform::identity(), None);
                    if self.magnifier.show_grid {
                        grid_path.push_rect(rect);
                    }
                }
            }
        }
        if self.magnifier.show_grid {
            if let Some(path) = grid_path.finish() {
                let mut grid_paint = Paint::default();
                grid_paint.set_color_rgba8(0, 0, 0, 35);
                let stroke = Stroke {
                    width: 0.5,
                    ..Default::default()
                };
                self.pixmap
                    .stroke_path(&path, &grid_paint, &stroke, Transform::identity(), None);
            }
        }
    }

    /// 绘制中心十字瞄准准星。
    fn render_center_reticle(&mut self) {
        let center_x = self.layout.target_x + SAMPLE_RADIUS as f32 * self.layout.cell_size;
        let center_y = self.layout.target_y + SAMPLE_RADIUS as f32 * self.layout.cell_size;
        let Some(rect) = SkiaRect::from_xywh(
            center_x,
            center_y,
            self.layout.cell_size,
            self.layout.cell_size,
        ) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 174, 255, 255);
        let stroke = Stroke {
            width: 2.0 * self.magnifier.ui_scale,
            ..Default::default()
        };
        let mut builder = PathBuilder::new();
        builder.push_rect(rect);
        if let Some(path) = builder.finish() {
            self.pixmap
                .stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }

    /// 绘制卡片外框与底部色值展示背景。
    fn render_frame(&mut self) {
        let Some(border_rect) = SkiaRect::from_xywh(
            self.layout.target_x,
            self.layout.target_y,
            self.layout.box_w,
            self.layout.box_h + self.layout.bottom_bar_h,
        ) else {
            return;
        };
        let mut background = Paint::default();
        background.set_color_rgba8(20, 20, 20, 220);
        if let Some(text_bg) = SkiaRect::from_xywh(
            self.layout.target_x,
            self.layout.target_y + self.layout.box_h,
            self.layout.box_w,
            self.layout.bottom_bar_h,
        ) {
            self.pixmap
                .fill_rect(text_bg, &background, Transform::identity(), None);
        }
        let mut outline = Paint::default();
        outline.set_color_rgba8(80, 80, 80, 255);
        let stroke = Stroke {
            width: 2.0 * self.magnifier.ui_scale,
            ..Default::default()
        };
        let mut builder = PathBuilder::new();
        builder.push_rect(border_rect);
        if let Some(path) = builder.finish() {
            self.pixmap
                .stroke_path(&path, &outline, &stroke, Transform::identity(), None);
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

    /// 卡片脏区必须完整包住绘制内容，并在光标靠近屏幕右下角时翻转到光标左上侧
    #[test]
    fn test_card_bounds_layout_and_flip() {
        let mag = Magnifier {
            enabled: true,
            scale: 8,
            format: ColorFormat::Hex,
            show_grid: true,
            ui_scale: 1.0,
        };

        // 采样 15 格 x 每格 8 像素 = 120，偏移 20，描边余量 2，底部色值条 24
        let normal = mag
            .card_bounds(MagnifierViewport {
                cursor_x: 100,
                cursor_y: 100,
                width: 1920,
                height: 1080,
            })
            .expect("启用时必须有布局");
        assert_eq!(normal.x, 118.0);
        assert_eq!(normal.y, 118.0);
        assert_eq!(normal.width, 124.0);
        assert_eq!(normal.height, 148.0);

        // 右下角空间不足时翻转到光标左上侧，且不得越出屏幕左/上边界
        let flipped = mag
            .card_bounds(MagnifierViewport {
                cursor_x: 1900,
                cursor_y: 1070,
                width: 1920,
                height: 1080,
            })
            .expect("启用时必须有布局");
        assert_eq!(flipped.x, 1758.0);
        assert_eq!(flipped.y, 908.0);

        // 禁用放大镜时既不绘制也没有脏区
        let disabled = Magnifier {
            enabled: false,
            ..mag
        };
        assert!(disabled
            .card_bounds(MagnifierViewport {
                cursor_x: 100,
                cursor_y: 100,
                width: 1920,
                height: 1080
            })
            .is_none());
    }
}
