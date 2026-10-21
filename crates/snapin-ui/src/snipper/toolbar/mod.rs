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

pub(crate) use items::CORNER_RADIUS_PRESETS;
pub use items::{preset_colors, StrokeWidthLevel, SubToolOptions, ToolbarItem};

use crate::snipper::selection::Rect;
use interaction::hit_test_main_items;
pub(crate) use layout::SubControl;
use layout::{calculate_main_layout, calculate_sub_layout, SubDropdown, SubLayout};
use render::render_toolbar;
use snapin_core::annotation::{ArrowStyle, BadgeStyle, LineStyle};
use tiny_skia::PixmapMut;

/// 工具栏整体状态与布局管理器
#[derive(Debug, Clone)]
pub struct Toolbar {
    /// 界面缩放倍率 (HiDPI 缩放因子)
    pub ui_scale: f32,
    /// 主栏按钮序列。
    ///
    /// 默认使用截图工具栏的完整按钮集；贴图窗口可注入自身的精简列表，
    /// 从而在不复制布局、命中与渲染代码的前提下复用同一套工具栏实现。
    /// 使用 `&'static` 切片是为了让工具栏保持轻量可克隆，且按钮序列在运行期恒定不变。
    pub main_items: &'static [ToolbarItem],
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
    /// 二级栏完整布局：控件热区与下拉菜单，渲染与命中共用
    sub_layout: Option<SubLayout>,
    /// 当前展开的下拉菜单
    pub(crate) open_dropdown: Option<SubDropdown>,
    /// 下拉菜单当前悬停行
    pub(crate) dropdown_hover: Option<usize>,
    /// 屏幕物理尺寸，用于下拉菜单避让
    screen_size: (f32, f32),
}

impl Default for Toolbar {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            main_items: Self::MAIN_ITEMS,
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
            sub_layout: None,
            open_dropdown: None,
            dropdown_hover: None,
            screen_size: (0.0, 0.0),
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
        self.screen_size = (screen_w, screen_h);
        self.main_rect = calculate_main_layout(
            selection,
            screen_w,
            screen_h,
            self.ui_scale,
            self.main_items,
        );
        self.update_sub_rect();
    }

    /// 主栏面板宽度（由当前 `main_items` 与界面缩放倍率共同决定）。
    ///
    /// 贴图窗口需要把工具栏锚定在图像内部，必须提前知道面板宽度才能摆放；
    /// 与命中测试、渲染共用同一份度量函数，避免三处坐标漂移。
    pub(crate) fn main_width(&self) -> f32 {
        metrics::main_toolbar_width(self.main_items, self.ui_scale)
    }

    /// 主栏面板高度（含上下内边距）。
    pub(crate) fn main_height(&self) -> f32 {
        metrics::padding(self.ui_scale) * 2.0 + metrics::button_size(self.ui_scale)
    }

    /// 根据当前主工具栏位置重新定位二级属性栏 (自适应 HiDPI 缩放)
    fn update_sub_rect(&mut self) {
        self.sub_layout = calculate_sub_layout(
            &self.main_rect,
            self.active_tool,
            self.open_dropdown,
            self.screen_size.0,
            self.screen_size.1,
            self.ui_scale,
        );
        self.sub_rect = self.sub_layout.as_ref().map(|layout| layout.rect);
        if self.sub_layout.is_none() {
            self.open_dropdown = None;
            self.dropdown_hover = None;
        }
    }

    /// 当前二级栏布局（供渲染层读取控件热区）
    pub(crate) fn sub_layout(&self) -> Option<&SubLayout> {
        self.sub_layout.as_ref()
    }

    /// 切换当前工具并收敛下拉菜单状态
    fn set_active_tool(&mut self, tool: Option<ToolbarItem>) {
        if self.active_tool != tool {
            self.open_dropdown = None;
            self.dropdown_hover = None;
        }
        self.active_tool = tool;
        self.update_sub_rect();
    }

    /// 应用下拉菜单中选中的行
    pub fn apply_dropdown_row(&mut self, row: usize) {
        match self.open_dropdown {
            Some(SubDropdown::Line) => {
                if let Some(style) = LineStyle::ALL.get(row) {
                    self.sub_options.line_style = *style;
                }
            }
            Some(SubDropdown::Badge) => {
                if row == 0 {
                    self.sub_options.badge_style = BadgeStyle::Filled;
                } else if row == 1 {
                    self.sub_options.badge_style = BadgeStyle::Outline;
                }
            }
            Some(SubDropdown::Arrow) => {
                if let Some(style) = ArrowStyle::ALL.get(row) {
                    self.sub_options.arrow_style = *style;
                }
            }
            Some(SubDropdown::Corner) => {
                if let Some(radius) = CORNER_RADIUS_PRESETS.get(row) {
                    self.sub_options.corner_radius = *radius;
                }
            }
            None => {}
        }
        self.open_dropdown = None;
        self.dropdown_hover = None;
        self.update_sub_rect();
    }

    /// 鼠标移动命中检测（主栏按钮与下拉菜单行）
    pub fn handle_mouse_move(&mut self, px: f32, py: f32) -> bool {
        let prev_item = self.hovered_item;
        let prev_row = self.dropdown_hover;
        self.hovered_item = self.hit_test_main(px, py);
        self.dropdown_hover = self
            .sub_layout
            .as_ref()
            .and_then(|layout| layout.dropdown_row_hit(px, py));
        prev_item != self.hovered_item || prev_row != self.dropdown_hover
    }

    /// 主栏命中测试 (自适应 HiDPI 缩放)
    pub fn hit_test_main(&self, px: f32, py: f32) -> Option<ToolbarItem> {
        hit_test_main_items(&self.main_rect, self.main_items, self.ui_scale, px, py)
    }

    /// 检查指定坐标是否落在主栏、二级栏或展开的下拉菜单内（防止事件穿透到画布）
    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        if self.main_rect.contains(px, py) {
            return true;
        }
        match &self.sub_layout {
            Some(layout) => layout.rect.contains(px, py) || layout.dropdown_contains(px, py),
            None => false,
        }
    }

    /// 点击项目切换工具、调整样式或触发动作
    pub fn handle_click(&mut self, px: f32, py: f32) -> Option<ToolbarItem> {
        // 1. 下拉菜单优先：命中选项行直接应用；点在面板空白处吞掉；点在面板外则先收起再继续
        let dropdown_row = self
            .sub_layout
            .as_ref()
            .and_then(|layout| layout.dropdown_row_hit(px, py));
        if let Some(row) = dropdown_row {
            self.apply_dropdown_row(row);
            return None;
        }
        if self
            .sub_layout
            .as_ref()
            .is_some_and(|layout| layout.dropdown_contains(px, py))
        {
            return None;
        }
        if self.open_dropdown.is_some() {
            self.open_dropdown = None;
            self.dropdown_hover = None;
            self.update_sub_rect();
        }

        // 2. 主栏按钮
        if let Some(item) = self.hit_test_main(px, py) {
            match item {
                ToolbarItem::ToolRect => {
                    // 形状按钮只做开合：点击选中矩形（默认形状），再点取消。
                    // 椭圆是同一个工具的第二形态，只能从二级属性栏显式选择——
                    // 旧实现在此循环"矩形→椭圆→关闭"，导致用户想点开选项时反而被切成椭圆。
                    let shape_active = matches!(
                        self.active_tool,
                        Some(ToolbarItem::ToolRect | ToolbarItem::ToolEllipse)
                    );
                    self.set_active_tool(if shape_active {
                        None
                    } else {
                        Some(ToolbarItem::ToolRect)
                    });
                    return self.active_tool;
                }
                ToolbarItem::ToolEllipse
                | ToolbarItem::ToolArrow
                | ToolbarItem::ToolPen
                | ToolbarItem::ToolMarker
                | ToolbarItem::ToolText
                | ToolbarItem::ToolBadge
                | ToolbarItem::ToolMosaic => {
                    let next = if self.active_tool == Some(item) {
                        None
                    } else {
                        Some(item)
                    };
                    self.set_active_tool(next);
                    return self.active_tool;
                }
                action => return Some(action),
            }
        }

        // 3. 二级栏控件
        let control = self
            .sub_layout
            .as_ref()
            .and_then(|layout| layout.hit(px, py));
        match control {
            Some(SubControl::ShapeRect) => self.set_active_tool(Some(ToolbarItem::ToolRect)),
            Some(SubControl::ShapeEllipse) => self.set_active_tool(Some(ToolbarItem::ToolEllipse)),
            Some(SubControl::Stroke(level)) => {
                self.sub_options.stroke_width = level;
                self.sub_options.custom_stroke_width = level.to_pixel();
            }
            Some(SubControl::LineStyle) => {
                self.open_dropdown = if self.open_dropdown == Some(SubDropdown::Line) {
                    None
                } else {
                    Some(SubDropdown::Line)
                };
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::FillToggle) => {
                self.sub_options.filled = !self.sub_options.filled;
            }
            // 圆角半径：单击展开预设档位，悬停其上滚动滚轮可连续微调
            Some(SubControl::CornerRadius) => {
                self.open_dropdown = if self.open_dropdown == Some(SubDropdown::Corner) {
                    None
                } else {
                    Some(SubDropdown::Corner)
                };
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::BadgeStyle) => {
                self.open_dropdown = if self.open_dropdown == Some(SubDropdown::Badge) {
                    None
                } else {
                    Some(SubDropdown::Badge)
                };
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::ArrowStyle) => {
                self.open_dropdown = if self.open_dropdown == Some(SubDropdown::Arrow) {
                    None
                } else {
                    Some(SubDropdown::Arrow)
                };
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::Color(index)) => {
                if let Some(color) = preset_colors().get(index) {
                    self.sub_options.color = *color;
                }
            }
            None => {}
        }

        None
    }

    /// 使用 tiny-skia 将工具栏绘制到目标 Pixmap
    pub fn render(&self, pixmap: &mut PixmapMut) {
        render_toolbar(self, pixmap);
    }
}
