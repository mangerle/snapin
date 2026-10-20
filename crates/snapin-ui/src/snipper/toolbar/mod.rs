//! 截图工具栏与二级属性选择器。
//!
//! 提供矩形、箭头、画笔、荧光笔、马赛克、文本等标注工具切换，
//! 以及撤销、重做、定在屏幕(贴图)、保存、复制、关闭等动作。
//! 支持自适应屏幕边缘检测，防止超出可视区域。

pub mod icons;
mod interaction;
pub mod items;
mod layout;
mod metrics;
pub mod render;

#[cfg(test)]
mod tests;

pub use items::{preset_colors, StrokeWidthLevel, SubToolOptions, ToolbarItem};

use crate::snipper::selection::Rect;
use interaction::{cycle_stroke_width_level, handle_sub_click, hit_test_main_items};
use layout::{calculate_main_layout, calculate_sub_layout};
use render::render_toolbar;
use tiny_skia::PixmapMut;

/// 工具栏整体状态与布局管理器
#[derive(Debug, Clone)]
pub struct Toolbar {
    /// 界面缩放倍率 (HiDPI 缩放因子)
    pub ui_scale: f32,
    /// 当前处于激活状态的标注工具 (None 表示未进入标注模式)
    pub active_tool: Option<ToolbarItem>,
    /// 当前鼠标悬停的项目
    pub hovered_item: Option<ToolbarItem>,
    /// 二级属性
    pub sub_options: SubToolOptions,
    /// 主工具栏当前在屏幕上的包围盒
    pub main_rect: Rect,
    /// 二级属性栏当前在屏幕上的包围盒 (若激活)
    pub sub_rect: Option<Rect>,
}

impl Default for Toolbar {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            active_tool: None,
            hovered_item: None,
            sub_options: SubToolOptions::default(),
            main_rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            sub_rect: None,
        }
    }
}

impl Toolbar {
    /// 主工具列表定义 (严格对齐经典工具栏规范)
    pub const MAIN_ITEMS: &'static [ToolbarItem] = &[
        ToolbarItem::ToolRect,        // 0: 几何形状 (矩形+椭圆)
        ToolbarItem::ToolBadge,       // 1: 序号步骤印章 (圆圈数字1)
        ToolbarItem::ToolArrow,       // 2: 折线箭头 (带颜色指示圆点)
        ToolbarItem::ToolPen,         // 3: 铅笔画笔
        ToolbarItem::ToolMarker,      // 4: 荧光记号笔
        ToolbarItem::ToolMosaic,      // 5: 黑白四宫格马赛克
        ToolbarItem::ToolText,        // 6: 文字 T
        ToolbarItem::ActionUndo,      // 7: 撤销 (前置分隔线 1)
        ToolbarItem::ActionRedo,      // 8: 重做
        ToolbarItem::ActionOcr,       // 9: OCR 扫描提取纯文本
        ToolbarItem::ActionTranslate, // 10: 截图原位翻译
        ToolbarItem::ActionClose,     // 11: 关闭取消 X (前置分隔线 2)
        ToolbarItem::ActionPin,       // 12: 贴图置顶 (图钉)
        ToolbarItem::ActionSave,      // 13: 保存文件 (软盘)
        ToolbarItem::ActionCopy,      // 14: 复制到剪贴板 (双重叠矩形)
    ];

    /// 根据选区与屏幕宽高重新计算工具栏自适应摆放位置 (自适应 HiDPI 缩放)
    pub fn update_layout(&mut self, selection: &Rect, screen_w: f32, screen_h: f32) {
        self.main_rect = calculate_main_layout(
            selection,
            screen_w,
            screen_h,
            self.ui_scale,
            Self::MAIN_ITEMS.len(),
        );
        self.update_sub_rect();
    }

    /// 根据当前主工具栏位置重新定位二级属性栏 (自适应 HiDPI 缩放)
    fn update_sub_rect(&mut self) {
        self.sub_rect = calculate_sub_layout(&self.main_rect, self.active_tool, self.ui_scale);
    }

    /// 鼠标移动命中检测
    pub fn handle_mouse_move(&mut self, px: f32, py: f32) -> bool {
        let prev = self.hovered_item;
        self.hovered_item = self.hit_test_main(px, py);
        prev != self.hovered_item
    }

    /// 主栏命中测试 (自适应 HiDPI 缩放)
    pub fn hit_test_main(&self, px: f32, py: f32) -> Option<ToolbarItem> {
        hit_test_main_items(&self.main_rect, Self::MAIN_ITEMS, self.ui_scale, px, py)
    }

    /// 检查指定坐标是否落在主工具栏或二级属性栏范围内 (防止事件穿透到画布)
    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        if self.main_rect.contains(px, py) {
            return true;
        }
        if let Some(sub) = self.sub_rect {
            if sub.contains(px, py) {
                return true;
            }
        }
        false
    }

    /// 点击项目切换工具或触发动作
    pub fn handle_click(&mut self, px: f32, py: f32) -> Option<ToolbarItem> {
        if let Some(item) = self.hit_test_main(px, py) {
            match item {
                ToolbarItem::ToolRect => {
                    if self.active_tool == Some(ToolbarItem::ToolRect) {
                        self.active_tool = Some(ToolbarItem::ToolEllipse);
                    } else if self.active_tool == Some(ToolbarItem::ToolEllipse) {
                        self.active_tool = None;
                    } else {
                        self.active_tool = Some(ToolbarItem::ToolRect);
                    }
                    self.update_sub_rect();
                    return self.active_tool;
                }
                ToolbarItem::ToolEllipse
                | ToolbarItem::ToolArrow
                | ToolbarItem::ToolPen
                | ToolbarItem::ToolMarker
                | ToolbarItem::ToolText
                | ToolbarItem::ToolBadge
                | ToolbarItem::ToolMosaic => {
                    if self.active_tool == Some(item) {
                        self.active_tool = None;
                    } else {
                        self.active_tool = Some(item);
                    }
                    self.update_sub_rect();
                    return Some(item);
                }
                action => return Some(action),
            }
        }

        if let Some(sub_rect) = self.sub_rect {
            if sub_rect.contains(px, py) {
                handle_sub_click(
                    sub_rect,
                    &mut self.active_tool,
                    &mut self.sub_options,
                    self.ui_scale,
                    px,
                    py,
                );
            }
        }

        None
    }

    /// 根据鼠标滚轮滚动方向阶梯调节标注粗细 (向前滚轮变粗，向后滚轮变细)
    pub fn cycle_stroke_width(&mut self, increase: bool) -> bool {
        let (next, changed) = cycle_stroke_width_level(self.sub_options.stroke_width, increase);
        if changed {
            self.sub_options.stroke_width = next;
        }
        changed
    }

    /// 使用 tiny-skia 将工具栏绘制到目标 Pixmap
    pub fn render(&self, pixmap: &mut PixmapMut) {
        render_toolbar(self, pixmap);
    }
}
