//! 截图工具栏与二级属性选择器。
//!
//! 提供矩形、箭头、画笔、荧光笔、马赛克、文本等标注工具切换，
//! 以及撤销、重做、定在屏幕(贴图)、保存、复制、关闭等动作。
//! 支持自适应屏幕边缘检测，防止超出可视区域。

pub mod icons;
pub mod items;
pub mod render;

pub use items::{preset_colors, StrokeWidthLevel, SubToolOptions, ToolbarItem};

use crate::snipper::selection::Rect;
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
    /// 步骤序号累加计数器 (用于 ToolBadge)
    pub badge_counter: usize,
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
            badge_counter: 1,
        }
    }
}

impl Toolbar {
    /// 主工具列表定义 (严格对齐经典工具栏规范)
    pub const MAIN_ITEMS: &'static [ToolbarItem] = &[
        ToolbarItem::ToolRect,    // 0: 几何形状 (矩形+椭圆)
        ToolbarItem::ToolArrow,   // 1: 折线箭头 (带颜色指示圆点)
        ToolbarItem::ToolPen,     // 2: 铅笔画笔
        ToolbarItem::ToolMarker,  // 3: 荧光记号笔
        ToolbarItem::ToolMosaic,  // 4: 黑白四宫格马赛克
        ToolbarItem::ToolText,    // 5: 文字 T
        ToolbarItem::ToolBadge,   // 6: 序号气泡/印章
        ToolbarItem::ActionUndo,  // 7: 撤销 (前置分隔线 1)
        ToolbarItem::ActionRedo,  // 8: 重做
        ToolbarItem::ActionClose, // 9: 关闭取消 X (前置分隔线 2)
        ToolbarItem::ActionPin,   // 10: 贴图置顶 (图钉)
        ToolbarItem::ActionSave,  // 11: 保存文件 (软盘)
        ToolbarItem::ActionCopy,  // 12: 复制到剪贴板 (双重叠矩形)
    ];

    /// 根据选区与屏幕宽高重新计算工具栏自适应摆放位置 (自适应 HiDPI 缩放)
    pub fn update_layout(&mut self, selection: &Rect, screen_w: f32, screen_h: f32) {
        let s = self.ui_scale;
        let btn_size = 32.0 * s;
        let padding = 6.0 * s;
        let spacing = 4.0 * s;
        let divider_width = 8.0 * s;

        let total_items = Self::MAIN_ITEMS.len() as f32;
        let width = padding * 2.0
            + total_items * btn_size
            + (total_items - 1.0) * spacing
            + divider_width * 2.0;
        let height = padding * 2.0 + btn_size;

        let mut x = (selection.x + selection.width) - width;
        let mut y = selection.y + selection.height + 8.0 * s;

        if x + width > screen_w - 8.0 * s {
            x = screen_w - width - 8.0 * s;
        }
        if x < 8.0 * s {
            x = 8.0 * s;
        }

        if y + height > screen_h - 8.0 * s {
            let above_y = selection.y - height - 8.0 * s;
            if above_y >= 8.0 * s {
                y = above_y;
            } else {
                y = (selection.y + selection.height - height - 8.0 * s).max(8.0 * s);
            }
        }

        self.main_rect = Rect {
            x,
            y,
            width,
            height,
        };

        self.update_sub_rect();
    }

    /// 根据当前主工具栏位置重新定位二级属性栏 (自适应 HiDPI 缩放)
    fn update_sub_rect(&mut self) {
        if self.active_tool.is_some() && self.main_rect.width > 0.0 {
            let s = self.ui_scale;
            let sub_h = 36.0 * s;
            let sub_w = 340.0 * s;
            let sub_x = self.main_rect.x;
            let sub_y = if self.main_rect.y >= sub_h + 8.0 * s {
                self.main_rect.y - sub_h - 4.0 * s
            } else {
                self.main_rect.y + self.main_rect.height + 4.0 * s
            };
            self.sub_rect = Some(Rect {
                x: sub_x,
                y: sub_y,
                width: sub_w,
                height: sub_h,
            });
        } else {
            self.sub_rect = None;
        }
    }

    /// 鼠标移动命中检测
    pub fn handle_mouse_move(&mut self, px: f32, py: f32) -> bool {
        let prev = self.hovered_item;
        self.hovered_item = self.hit_test_main(px, py);
        prev != self.hovered_item
    }

    /// 主栏命中测试 (自适应 HiDPI 缩放)
    pub fn hit_test_main(&self, px: f32, py: f32) -> Option<ToolbarItem> {
        if !self.main_rect.contains(px, py) {
            return None;
        }

        let s = self.ui_scale;
        let btn_size = 32.0 * s;
        let padding = 6.0 * s;
        let spacing = 4.0 * s;
        let mut cur_x = self.main_rect.x + padding;

        for (idx, &item) in Self::MAIN_ITEMS.iter().enumerate() {
            if idx == 7 || idx == 9 {
                cur_x += 8.0 * s;
            }

            let btn_rect = Rect {
                x: cur_x,
                y: self.main_rect.y + padding,
                width: btn_size,
                height: btn_size,
            };

            if btn_rect.contains(px, py) {
                return Some(item);
            }

            cur_x += btn_size + spacing;
        }

        None
    }

    /// 点击项目切换工具或触发动作
    pub fn handle_click(&mut self, px: f32, py: f32) -> Option<ToolbarItem> {
        if let Some(item) = self.hit_test_main(px, py) {
            match item {
                ToolbarItem::ToolRect
                | ToolbarItem::ToolEllipse
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
                self.handle_sub_click(px, py, sub_rect);
            }
        }

        None
    }

    /// 处理二级属性栏内的粗细与颜色点击 (自适应 HiDPI 缩放)
    fn handle_sub_click(&mut self, px: f32, py: f32, sub_rect: Rect) {
        let s = self.ui_scale;
        let padding = 8.0 * s;
        let mut cur_x = sub_rect.x + padding;
        let center_y = sub_rect.y + sub_rect.height / 2.0;

        for level in [
            StrokeWidthLevel::Thin,
            StrokeWidthLevel::Medium,
            StrokeWidthLevel::Thick,
        ] {
            let hit_box = Rect {
                x: cur_x - 6.0 * s,
                y: center_y - 12.0 * s,
                width: 24.0 * s,
                height: 24.0 * s,
            };
            if hit_box.contains(px, py) {
                self.sub_options.stroke_width = level;
                return;
            }
            cur_x += 24.0 * s;
        }

        cur_x += 16.0 * s;

        for color in preset_colors() {
            let hit_box = Rect {
                x: cur_x - 4.0 * s,
                y: center_y - 12.0 * s,
                width: 24.0 * s,
                height: 24.0 * s,
            };
            if hit_box.contains(px, py) {
                self.sub_options.color = color;
                return;
            }
            cur_x += 26.0 * s;
        }
    }

    /// 根据鼠标滚轮滚动方向阶梯调节标注粗细 (向前滚轮变粗，向后滚轮变细)
    pub fn cycle_stroke_width(&mut self, increase: bool) -> bool {
        let prev = self.sub_options.stroke_width;
        let next = if increase {
            match prev {
                StrokeWidthLevel::Thin => StrokeWidthLevel::Medium,
                StrokeWidthLevel::Medium | StrokeWidthLevel::Thick => StrokeWidthLevel::Thick,
            }
        } else {
            match prev {
                StrokeWidthLevel::Thick => StrokeWidthLevel::Medium,
                StrokeWidthLevel::Medium | StrokeWidthLevel::Thin => StrokeWidthLevel::Thin,
            }
        };

        if next != prev {
            self.sub_options.stroke_width = next;
            true
        } else {
            false
        }
    }

    /// 使用 tiny-skia 将工具栏绘制到目标 Pixmap
    pub fn render(&self, pixmap: &mut PixmapMut) {
        render_toolbar(self, pixmap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolbar_layout_adaptation() {
        let mut tb = Toolbar::default();
        let selection = Rect {
            x: 100.0,
            y: 100.0,
            width: 500.0,
            height: 400.0,
        };
        tb.update_layout(&selection, 1920.0, 1080.0);
        assert!(tb.main_rect.x > 0.0);
        assert!(tb.main_rect.y >= selection.y + selection.height);

        let bottom_selection = Rect {
            x: 100.0,
            y: 900.0,
            width: 500.0,
            height: 160.0,
        };
        tb.update_layout(&bottom_selection, 1920.0, 1080.0);
        assert!(tb.main_rect.y + tb.main_rect.height <= 1080.0);
    }

    #[test]
    fn test_toolbar_hit_test() {
        let mut tb = Toolbar::default();
        let selection = Rect {
            x: 200.0,
            y: 200.0,
            width: 600.0,
            height: 400.0,
        };
        tb.update_layout(&selection, 1920.0, 1080.0);

        assert_eq!(tb.hit_test_main(0.0, 0.0), None);

        let first_btn_x = tb.main_rect.x + 10.0;
        let first_btn_y = tb.main_rect.y + 10.0;
        let clicked = tb.handle_click(first_btn_x, first_btn_y);
        assert_eq!(clicked, Some(ToolbarItem::ToolRect));
        assert_eq!(tb.active_tool, Some(ToolbarItem::ToolRect));
        assert!(tb.sub_rect.is_some());
    }

    #[test]
    fn test_toolbar_cycle_stroke_width() {
        let mut tb = Toolbar::default();
        assert_eq!(tb.sub_options.stroke_width, StrokeWidthLevel::Medium);

        // 滚轮向上增大粗细：Medium -> Thick
        assert!(tb.cycle_stroke_width(true));
        assert_eq!(tb.sub_options.stroke_width, StrokeWidthLevel::Thick);
        // 到顶不再变化
        assert!(!tb.cycle_stroke_width(true));

        // 滚轮向下减小粗细：Thick -> Medium -> Thin
        assert!(tb.cycle_stroke_width(false));
        assert_eq!(tb.sub_options.stroke_width, StrokeWidthLevel::Medium);
        assert!(tb.cycle_stroke_width(false));
        assert_eq!(tb.sub_options.stroke_width, StrokeWidthLevel::Thin);
        // 到底不再变化
        assert!(!tb.cycle_stroke_width(false));
    }
}
