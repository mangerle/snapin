//! 贴图窗口运行时状态封装与窗口尺寸缩放管理。

use crate::paster::context_menu::ContextMenuState;
use crate::paster::manager::PasterItem;
use crate::paster::window::annotation::{create_paster_toolbar, screen_to_image_point};
use crate::snipper::toolbar::{Toolbar, ToolbarItem};
use crate::snipper::window::annotation_builder::{build_shape, AnnotationDraftParams};
use snapin_core::annotation::AnnotationHistory;
use snapin_core::config::{Config, ZoomAnchor};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tiny_skia::Pixmap;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::window::Window;

/// 贴图窗口运行时状态
pub struct PasterWindowState {
    pub item: PasterItem,
    /// 创建贴图时的原始像素副本，撤销/重做时作为重放标注的唯一底图
    pub base_pixmap: Pixmap,
    /// 已提交标注图元的撤销/重做历史（坐标为图像像素坐标）
    pub annotations: AnnotationHistory,
    /// 贴图标注工具栏（主栏按钮集由 `PASTER_MAIN_ITEMS` 指定）
    pub toolbar: Toolbar,
    pub should_close: bool,
    pub has_shadow: bool,
    pub show_toolbar: bool,
    pub smooth_scaling: bool,
    pub file_path: Option<PathBuf>,
    pub context_menu: ContextMenuState,
    pub cursor_pos: (f32, f32),
    pub ui_scale: f32,
    pub config: Arc<Config>,
    pub(crate) last_click_time: Option<Instant>,
    /// 当前一笔尚待提交的标注轨迹（图像像素坐标）
    pub(crate) pending_stroke_points: Vec<(f32, f32)>,
    /// 左键是否正处于一笔标注的拖拽过程中
    pub(crate) is_drawing: bool,
}

impl PasterWindowState {
    pub fn new(item: PasterItem) -> Self {
        let base_pixmap = item.pixmap.clone();
        Self {
            item,
            base_pixmap,
            annotations: AnnotationHistory::default(),
            toolbar: create_paster_toolbar(),
            should_close: false,
            has_shadow: true,
            show_toolbar: false,
            smooth_scaling: true,
            file_path: None,
            context_menu: ContextMenuState::default(),
            cursor_pos: (0.0, 0.0),
            ui_scale: 1.0,
            config: Arc::new(Config::default()),
            last_click_time: None,
            pending_stroke_points: Vec::with_capacity(64),
            is_drawing: false,
        }
    }

    /// 根据用户配置初始化贴图状态（注入初始不透明度）
    pub fn from_config(mut item: PasterItem, config: Arc<Config>) -> Self {
        item.transform.opacity = config.paster.initial_opacity.clamp(0.1, 1.0);
        let mut state = Self::new(item);
        state.config = config;
        state
    }

    /// 顺时针旋转 90 度并同步调整物理窗口尺寸
    pub fn rotate_cw(&mut self, window: &Window) -> bool {
        if let Some(rotated) = crate::paster::transform::rotate_pixmap_cw(&self.item.pixmap) {
            self.item.pixmap = rotated;
            self.reset_annotation_baseline();
            apply_window_scale_resize(self, window);
            true
        } else {
            false
        }
    }

    /// 逆时针旋转 90 度并同步调整物理窗口尺寸
    pub fn rotate_ccw(&mut self, window: &Window) -> bool {
        if let Some(rotated) = crate::paster::transform::rotate_pixmap_ccw(&self.item.pixmap) {
            self.item.pixmap = rotated;
            self.reset_annotation_baseline();
            apply_window_scale_resize(self, window);
            true
        } else {
            false
        }
    }

    /// 水平翻转图像
    pub fn flip_horizontal(&mut self) -> bool {
        if let Some(flipped) = crate::paster::transform::flip_pixmap_horizontal(&self.item.pixmap) {
            self.item.pixmap = flipped;
            self.reset_annotation_baseline();
            true
        } else {
            false
        }
    }

    /// 垂直翻转图像
    pub fn flip_vertical(&mut self) -> bool {
        if let Some(flipped) = crate::paster::transform::flip_pixmap_vertical(&self.item.pixmap) {
            self.item.pixmap = flipped;
            self.reset_annotation_baseline();
            true
        } else {
            false
        }
    }

    /// 设置缩放比例并同步调整物理窗口尺寸
    pub fn set_scale(&mut self, scale: f32, window: &Window) {
        self.item.transform.set_scale(scale);
        apply_window_scale_resize(self, window);
    }

    /// 滚轮平滑缩放，并按配置的光标/中心锚点重新定位窗口。
    pub fn zoom(&mut self, delta: f32, window: &Window) {
        let old_size = window.inner_size();
        let old_scale = self.item.transform.scale;
        self.item.transform.zoom(delta);
        let new_scale = self.item.transform.scale;
        if (new_scale - old_scale).abs() < f32::EPSILON {
            return;
        }

        let base_width = self.item.pixmap.width() as f32;
        let base_height = self.item.pixmap.height() as f32;
        let new_width = (base_width * new_scale).max(20.0);
        let new_height = (base_height * new_scale).max(20.0);
        let (ratio_x, ratio_y) = zoom_anchor_ratio(
            self.config.paster.zoom_anchor,
            self.cursor_pos,
            old_size.width as f32,
            old_size.height as f32,
        );

        if let Ok(position) = window.outer_position() {
            let delta_x = ((new_width - old_size.width as f32) * ratio_x).round() as i32;
            let delta_y = ((new_height - old_size.height as f32) * ratio_y).round() as i32;
            window.set_outer_position(PhysicalPosition::new(
                position.x - delta_x,
                position.y - delta_y,
            ));
        }

        apply_window_scale_resize(self, window);
    }

    /// 微调不透明度
    pub fn adjust_opacity(&mut self, delta: f32) {
        self.item.transform.adjust_opacity(delta);
    }

    /// 更新贴图像素内容并同步调整物理窗口尺寸
    pub fn replace_pixmap(&mut self, pixmap: tiny_skia::Pixmap, window: &Window) {
        self.item.pixmap = pixmap;
        // 新图像意味着旧标注的坐标体系已完全失效，必须连同历史一起重置
        self.reset_annotation_baseline();
        apply_window_scale_resize(self, window);
    }

    /// 显示/隐藏标注工具栏；隐藏时退出绘制模式，避免用户在没有工具条提示时误画。
    pub fn set_toolbar_visible(&mut self, visible: bool) {
        self.show_toolbar = visible;
        if !visible {
            self.toolbar.active_tool = None;
            self.pending_stroke_points.clear();
            self.is_drawing = false;
        }
    }

    /// 判断屏幕坐标是否落在缩放后的图像内部（边框画在图像内部，不占布局，故直接用图像尺寸换算）。
    pub(crate) fn screen_point_inside_image(&self, pos: (f32, f32)) -> bool {
        crate::paster::window::annotation::is_point_inside_image(
            pos,
            self.item.transform.scale,
            self.item.pixmap.width(),
            self.item.pixmap.height(),
        )
    }

    /// 开始一笔标注：记录起点图像坐标，等待拖动更新与松开提交。
    pub(crate) fn begin_stroke(&mut self, screen_pos: (f32, f32)) -> bool {
        if self.toolbar.active_tool.is_none() {
            return false;
        }
        self.pending_stroke_points.clear();
        self.pending_stroke_points
            .push(self.image_point(screen_pos));
        self.is_drawing = true;
        true
    }

    /// 拖动中更新当前笔迹：画笔/荧光笔追加连续点，其余图元只保留起点与最新终点。
    pub(crate) fn update_stroke(&mut self, screen_pos: (f32, f32)) -> bool {
        if !self.is_drawing {
            return false;
        }
        let Some(tool) = self.toolbar.active_tool else {
            self.pending_stroke_points.clear();
            self.is_drawing = false;
            return false;
        };

        let point = self.image_point(screen_pos);
        match tool {
            ToolbarItem::ToolPen | ToolbarItem::ToolMarker => {
                if self.pending_stroke_points.last() != Some(&point) {
                    self.pending_stroke_points.push(point);
                }
            }
            _ => {
                if self.pending_stroke_points.len() > 1 {
                    self.pending_stroke_points[1] = point;
                } else {
                    self.pending_stroke_points.push(point);
                }
            }
        }
        true
    }

    /// 将当前待提交笔迹烘焙进 `item.pixmap` 并压入标注历史。
    ///
    /// 先绘制像素再入历史：复制图像、图像另存为都直接读取 `item.pixmap`，
    /// 因此天然携带已提交的标注；撤销/重做则依赖底图与历史重放恢复。
    pub(crate) fn commit_pending_stroke(&mut self) -> bool {
        self.is_drawing = false;
        let Some(tool) = self.toolbar.active_tool else {
            self.pending_stroke_points.clear();
            return false;
        };

        let shape = {
            let params = AnnotationDraftParams::from_toolbar_and_points(
                &self.toolbar,
                tool,
                &self.pending_stroke_points,
                self.annotations.next_badge_number(),
            );
            params.and_then(|params| build_shape(&params))
        };
        self.pending_stroke_points.clear();

        let Some(shape) = shape else {
            return false;
        };
        shape.draw(&mut self.item.pixmap.as_mut());
        self.annotations.push(shape);
        true
    }

    /// 撤销上一笔标注并基于底图重放剩余图元。
    pub fn undo_annotation(&mut self) -> bool {
        if self.annotations.undo() {
            self.rebuild_item_pixmap();
            true
        } else {
            false
        }
    }

    /// 重做下一笔标注并基于底图重放全部生效图元。
    pub fn redo_annotation(&mut self) -> bool {
        if self.annotations.redo() {
            self.rebuild_item_pixmap();
            true
        } else {
            false
        }
    }

    /// 屏幕坐标转图像坐标（使用当前缩放并夹取边界）
    fn image_point(&self, screen_pos: (f32, f32)) -> (f32, f32) {
        screen_to_image_point(
            screen_pos,
            self.item.transform.scale,
            self.item.pixmap.width(),
            self.item.pixmap.height(),
        )
    }

    /// 从原始底图重建展示图像，并按顺序重放当前生效的标注。
    fn rebuild_item_pixmap(&mut self) {
        let mut pixmap = self.base_pixmap.clone();
        self.annotations.replay_all(&mut pixmap.as_mut());
        self.item.pixmap = pixmap;
    }

    /// 将当前图像固化为新的标注底图并清空历史。
    ///
    /// 旋转/翻转/替换图像会改变像素坐标体系，历史图元在新坐标系中已失去意义
    /// （特别是旋转会交换宽高），因此必须整体重置而不能保留撤销记录。
    fn reset_annotation_baseline(&mut self) {
        self.base_pixmap = self.item.pixmap.clone();
        self.annotations.clear();
        self.pending_stroke_points.clear();
        self.is_drawing = false;
    }
}

/// 重新计算并应用根据缩放比例与右键菜单展开状态调整后的窗口尺寸
pub(crate) fn apply_window_scale_resize(state: &PasterWindowState, window: &Window) {
    let base_w = state.item.pixmap.width() as f32;
    let base_h = state.item.pixmap.height() as f32;
    let scale = state.item.transform.scale;
    let mut new_w = (base_w * scale).max(20.0);
    let mut new_h = (base_h * scale).max(20.0);

    // 若当前正在展示右键菜单，确保窗口物理尺寸足以容纳菜单而不被截断
    if state.context_menu.is_open {
        let main_items = ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
        let (req_w, req_h) = state.context_menu.required_menu_extent(&main_items);
        new_w = new_w.max(req_w);
        new_h = new_h.max(req_h);
    }

    let _ = window.request_inner_size(PhysicalSize::new(new_w as u32, new_h as u32));
}

/// 计算缩放时用于保持锚点稳定的相对位置比例。
///
/// - 光标锚点：返回光标在旧窗口尺寸中的相对比例；
/// - 中心锚点：返回 `(0.5, 0.5)`。
pub(crate) fn zoom_anchor_ratio(
    anchor: ZoomAnchor,
    cursor: (f32, f32),
    old_width: f32,
    old_height: f32,
) -> (f32, f32) {
    match anchor {
        ZoomAnchor::Cursor => (
            (cursor.0 / old_width.max(1.0)).clamp(0.0, 1.0),
            (cursor.1 / old_height.max(1.0)).clamp(0.0, 1.0),
        ),
        ZoomAnchor::Center => (0.5, 0.5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_anchor_ratio() {
        assert_eq!(
            zoom_anchor_ratio(ZoomAnchor::Cursor, (100.0, 50.0), 400.0, 100.0),
            (0.25, 0.5)
        );
        assert_eq!(
            zoom_anchor_ratio(ZoomAnchor::Center, (100.0, 50.0), 400.0, 100.0),
            (0.5, 0.5)
        );
        assert_eq!(
            zoom_anchor_ratio(ZoomAnchor::Cursor, (900.0, 900.0), 400.0, 100.0),
            (1.0, 1.0)
        );
    }
}
