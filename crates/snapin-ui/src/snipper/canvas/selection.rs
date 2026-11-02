//! 截图选区、窗口高亮、标注编辑框与待提交预览绘制。

use super::*;

impl<'a> SnipperCanvas<'a> {
    /// 通过直接内存行切片快速将原图像素恢复到选区内（亚毫秒级完成镂空透出）
    pub(super) fn fast_cutout_restore(
        &self,
        pixmap: &mut PixmapMut,
        rect: &Rect,
        canvas: CanvasSize,
    ) {
        let (px, py, pw, ph) = rect.to_pixel_bounds(canvas.width, canvas.height);
        if pw == 0 || ph == 0 {
            return;
        }

        let stride = (canvas.width as usize) * 4;
        let row_bytes = (pw as usize) * 4;
        let orig_data = self.original_rgba.as_raw();
        let dest_data = pixmap.data_mut();

        for row in 0..ph {
            let y_idx = (py + row) as usize;
            let start_offset = y_idx * stride + (px as usize) * 4;
            let end_offset = start_offset + row_bytes;
            dest_data[start_offset..end_offset]
                .copy_from_slice(&orig_data[start_offset..end_offset]);
        }
    }

    /// 绘制窗口截图悬停高亮：只画主题色边框，不显示缩放锚点
    pub(super) fn render_window_hover_border(&self, pixmap: &mut PixmapMut, rect: &Rect) {
        let Some(skia_rect) = SkiaRect::from_xywh(rect.x, rect.y, rect.width, rect.height) else {
            return;
        };
        let s = self.ui_scale;
        let (ar, ag, ab) = self.accent_color;
        let mut paint = Paint::default();
        paint.set_color_rgba8(ar, ag, ab, 255);
        let stroke = Stroke {
            width: 2.0 * s,
            ..Default::default()
        };
        let mut pb = PathBuilder::new();
        pb.push_rect(skia_rect);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }

    /// 绘制高亮线框与 8 个缩放手柄 (自适应 HiDPI 缩放)
    pub(super) fn render_selection_borders_and_handles(&self, pixmap: &mut PixmapMut, rect: &Rect) {
        let s = self.ui_scale;
        if let Some(skia_r) = SkiaRect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
            let (ar, ag, ab) = self.accent_color;
            let mut stroke_paint = Paint::default();
            stroke_paint.set_color_rgba8(ar, ag, ab, 255);
            let stroke = Stroke {
                width: 2.0 * s,
                ..Default::default()
            };

            let mut pb = PathBuilder::new();
            pb.push_rect(skia_r);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
            }
            render_handle_grid(
                pixmap,
                HandleGridSpec {
                    centers: Self::handle_positions(rect),
                    size: (6.0 * s).round().max(4.0),
                    accent: (ar, ag, ab),
                    scale: s,
                },
            );
        }
    }

    /// 实时渲染当前正在交互拖拽中的即时图元 (支持矩形、椭圆、箭头、画笔、记号笔、马赛克、序号实时动态伸缩)
    pub(super) fn render_pending_preview(
        &self,
        pixmap: &mut PixmapMut,
        spec: PendingPreviewSpec<'_>,
    ) {
        if let Some(params) = AnnotationDraftParams::from_toolbar_and_points(
            spec.toolbar,
            *spec.tool,
            spec.points,
            spec.badge_number,
        ) {
            render_draft_preview(&params, pixmap);
        }
    }

    /// 绘制已选中标注的编辑框：白色虚线包围盒与 8 个缩放手柄。
    ///
    /// 虚线样式用于与选区实线边框区分；不可拉伸的复合图元只绘制包围盒。
    pub(super) fn render_annotation_selection(
        &self,
        pixmap: &mut PixmapMut,
        selected: &SelectedAnnotation,
    ) {
        let Some(skia_rect) = SkiaRect::from_xywh(
            selected.rect.x,
            selected.rect.y,
            selected.rect.width,
            selected.rect.height,
        ) else {
            return;
        };
        let s = self.ui_scale;

        let mut dash_paint = Paint::default();
        dash_paint.set_color_rgba8(255, 255, 255, 235);
        let dash = Stroke {
            width: (1.2 * s).max(1.0),
            dash: StrokeDash::new(vec![6.0 * s, 4.0 * s], 0.0),
            ..Default::default()
        };
        let mut pb = PathBuilder::new();
        pb.push_rect(skia_rect);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &dash_paint, &dash, Transform::identity(), None);
        }

        if !selected.resizable {
            return;
        }

        let (ar, ag, ab) = self.accent_color;
        render_handle_grid(
            pixmap,
            HandleGridSpec {
                centers: Self::handle_positions(&selected.rect),
                size: (6.0 * s).round().max(4.0),
                accent: (ar, ag, ab),
                scale: s,
            },
        );
    }

    /// 计算包围盒 8 个控制手柄的中心坐标 (4 个角点 + 4 条边中点)
    fn handle_positions(rect: &Rect) -> [(f32, f32); 8] {
        [
            (rect.x, rect.y),
            (rect.x + rect.width / 2.0, rect.y),
            (rect.x + rect.width, rect.y),
            (rect.x + rect.width, rect.y + rect.height / 2.0),
            (rect.x + rect.width, rect.y + rect.height),
            (rect.x + rect.width / 2.0, rect.y + rect.height),
            (rect.x, rect.y + rect.height),
            (rect.x, rect.y + rect.height / 2.0),
        ]
    }
}

/// 一次标注草稿预览所需的工具、点集与序号参数。
pub(super) struct PendingPreviewSpec<'a> {
    pub toolbar: &'a Toolbar,
    pub tool: &'a Tool,
    pub points: &'a [crate::geometry::Point],
    pub badge_number: u32,
}

/// 缩放手柄网格的几何与主题参数。
struct HandleGridSpec {
    centers: [(f32, f32); 8],
    size: f32,
    accent: (u8, u8, u8),
    scale: f32,
}

/// 绘制 8 个白底主题色描边的缩放手柄。
fn render_handle_grid(pixmap: &mut PixmapMut, spec: HandleGridSpec) {
    let HandleGridSpec {
        centers,
        size,
        accent,
        scale,
    } = spec;
    let mut fill = Paint::default();
    fill.set_color_rgba8(255, 255, 255, 255);
    let mut border = Paint::default();
    border.set_color_rgba8(accent.0, accent.1, accent.2, 255);
    let stroke = Stroke {
        width: scale,
        ..Default::default()
    };
    for (cx, cy) in centers {
        let Some(rect) = SkiaRect::from_xywh(cx - size / 2.0, cy - size / 2.0, size, size) else {
            continue;
        };
        pixmap.fill_rect(rect, &fill, Transform::identity(), None);
        let mut path = PathBuilder::new();
        path.push_rect(rect);
        if let Some(path) = path.finish() {
            pixmap.stroke_path(&path, &border, &stroke, Transform::identity(), None);
        }
    }
}
