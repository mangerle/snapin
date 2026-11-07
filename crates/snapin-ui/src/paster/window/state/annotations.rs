//! 贴图窗口标注绘制、提交与撤销重做。

use super::PasterWindowState;
use crate::editor::annotation_builder::{build_shape, AnnotationDraftParams};
use crate::editor::toolbar::Tool;
use crate::geometry::Point;

impl PasterWindowState {
    /// 开始一笔标注：记录起点图像坐标，等待拖动更新与松开提交。
    pub(crate) fn begin_stroke(&mut self, screen_pos: (f32, f32)) -> bool {
        if self.interaction.annotation.toolbar.active_tool.is_none() {
            return false;
        }
        self.interaction
            .annotation
            .stroke
            .begin(Point::from(self.image_point(screen_pos)));
        true
    }

    /// 拖动中更新当前笔迹：画笔/荧光笔追加连续点，其余图元只保留起点与最新终点。
    pub(crate) fn update_stroke(&mut self, screen_pos: (f32, f32)) -> bool {
        if !self.interaction.annotation.stroke.is_drawing {
            return false;
        }
        let Some(tool) = self.interaction.annotation.toolbar.active_tool else {
            self.interaction.annotation.stroke.clear();
            return false;
        };

        let point = Point::from(self.image_point(screen_pos));
        if tool == Tool::Eraser
            && !self
                .interaction
                .annotation
                .toolbar
                .sub_options
                .eraser_rectangle_mode
        {
            if self.erase_annotations_at(point) {
                self.rebuild_item_pixmap();
            }
        } else {
            let is_brush_mosaic = self
                .interaction
                .annotation
                .toolbar
                .sub_options
                .mosaic_brush_mode;
            let is_polyline = self.interaction.annotation.toolbar.sub_options.line_mode
                == crate::editor::toolbar::LineMode::Polyline;
            self.interaction.annotation.stroke.update_with_tool(
                tool,
                point,
                is_brush_mosaic,
                is_polyline,
            );
        }
        true
    }

    /// 将当前待提交笔迹烘焙进 `item.pixmap` 并压入标注历史。
    ///
    /// 先绘制像素再入历史：复制图像、图像另存为都直接读取 `item.pixmap`，
    /// 因此天然携带已提交的标注；撤销/重做则依赖底图与历史重放恢复。
    pub(crate) fn commit_pending_stroke(&mut self) -> bool {
        self.interaction.annotation.stroke.is_drawing = false;
        let Some(tool) = self.interaction.annotation.toolbar.active_tool else {
            self.interaction.annotation.stroke.points.clear();
            return false;
        };
        if tool == Tool::Eraser {
            return self.commit_eraser_stroke();
        }
        self.commit_draw_stroke(tool)
    }

    /// 提交橡皮擦操作；矩形模式删除相交图元，画笔模式删除笔刷范围内图元。
    fn commit_eraser_stroke(&mut self) -> bool {
        let erase_changed = if self
            .interaction
            .annotation
            .toolbar
            .sub_options
            .eraser_rectangle_mode
        {
            self.interaction
                .annotation
                .stroke
                .points
                .first()
                .zip(self.interaction.annotation.stroke.points.last())
                .map(|(start, end)| crate::geometry::Rect::from_point_pair(*start, *end))
                .map(|rect| self.document.annotations.erase_intersecting(rect) > 0)
                .unwrap_or(false)
        } else {
            self.interaction
                .annotation
                .stroke
                .points
                .last()
                .copied()
                .map(|point| self.erase_annotations_at(point))
                .unwrap_or(false)
        };
        self.interaction.annotation.stroke.points.clear();
        if erase_changed {
            self.rebuild_item_pixmap();
        }
        erase_changed
    }

    /// 从当前笔迹构造图元并提交到图像与历史。
    fn commit_draw_stroke(&mut self, tool: Tool) -> bool {
        let shape = {
            let params = AnnotationDraftParams::from_toolbar_and_points(
                &self.interaction.annotation.toolbar,
                tool,
                &self.interaction.annotation.stroke.points,
                self.document.annotations.next_badge_number(),
            );
            params.and_then(|params| build_shape(&params))
        };
        self.interaction.annotation.stroke.points.clear();

        let Some(shape) = shape else {
            return false;
        };
        self.ensure_base_pixmap();
        shape.draw(&mut self.document.item.pixmap.as_mut());
        self.document.annotations.push(shape);
        true
    }

    /// 擦除画笔半径内命中的标注并立即重放剩余图元。
    fn erase_annotations_at(&mut self, point: Point) -> bool {
        let radius = self
            .interaction
            .annotation
            .toolbar
            .sub_options
            .custom_stroke_width
            .max(4.0);
        let rect = crate::geometry::Rect {
            x: point.x - radius,
            y: point.y - radius,
            width: radius * 2.0,
            height: radius * 2.0,
        };
        self.document.annotations.erase_intersecting(rect) > 0
    }

    /// 撤销上一笔标注并基于底图重放剩余图元。
    pub fn undo_annotation(&mut self) -> bool {
        if self.document.annotations.undo() {
            self.rebuild_item_pixmap();
            true
        } else {
            false
        }
    }

    /// 重做下一笔标注并基于底图重放全部生效图元。
    pub fn redo_annotation(&mut self) -> bool {
        if self.document.annotations.redo() {
            self.rebuild_item_pixmap();
            true
        } else {
            false
        }
    }
}
