//! 截图工具栏与二级属性选择器。
//!
//! 提供矩形、箭头、画笔、荧光笔、马赛克、文本等标注工具切换，
//! 以及撤销、重做、定在屏幕(贴图)、保存、复制、关闭等动作。
//! 支持自适应屏幕边缘检测，防止超出可视区域。

pub mod items;
pub mod render;

pub use items::{preset_colors, StrokeWidthLevel, SubToolOptions, ToolbarItem};

use crate::snipper::selection::Rect;
use render::render_toolbar;
use tiny_skia::PixmapMut;

/// 工具栏整体状态与布局管理器
#[derive(Debug, Clone)]
pub struct Toolbar {
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
    /// 主工具列表定义
    pub const MAIN_ITEMS: &'static [ToolbarItem] = &[
        ToolbarItem::ToolRect,
        ToolbarItem::ToolArrow,
        ToolbarItem::ToolPen,
        ToolbarItem::ToolMarker,
        ToolbarItem::ToolMosaic,
        ToolbarItem::ToolText,
        ToolbarItem::ActionUndo,
        ToolbarItem::ActionRedo,
        ToolbarItem::ActionPin,
        ToolbarItem::ActionSave,
        ToolbarItem::ActionCopy,
        ToolbarItem::ActionClose,
    ];

    /// 根据选区与屏幕宽高重新计算工具栏自适应摆放位置
    pub fn update_layout(&mut self, selection: &Rect, screen_w: f32, screen_h: f32) {
        let btn_size = 32.0;
        let padding = 6.0;
        let spacing = 4.0;
        let divider_width = 8.0;

        let total_items = Self::MAIN_ITEMS.len() as f32;
        let width = padding * 2.0
            + total_items * btn_size
            + (total_items - 1.0) * spacing
            + divider_width * 2.0;
        let height = padding * 2.0 + btn_size;

        let mut x = (selection.x + selection.width) - width;
        let mut y = selection.y + selection.height + 8.0;

        if x + width > screen_w - 8.0 {
            x = screen_w - width - 8.0;
        }
        if x < 8.0 {
            x = 8.0;
        }

        if y + height > screen_h - 8.0 {
            let above_y = selection.y - height - 8.0;
            if above_y >= 8.0 {
                y = above_y;
            } else {
                y = (selection.y + selection.height - height - 8.0).max(8.0);
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

    /// 根据当前主工具栏位置重新定位二级属性栏
    fn update_sub_rect(&mut self) {
        if self.active_tool.is_some() && self.main_rect.width > 0.0 {
            let sub_h = 36.0;
            let sub_w = 340.0;
            let sub_x = self.main_rect.x;
            let sub_y = if self.main_rect.y >= sub_h + 8.0 {
                self.main_rect.y - sub_h - 4.0
            } else {
                self.main_rect.y + self.main_rect.height + 4.0
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

    /// 主栏命中测试
    pub fn hit_test_main(&self, px: f32, py: f32) -> Option<ToolbarItem> {
        if !self.main_rect.contains(px, py) {
            return None;
        }

        let btn_size = 32.0;
        let padding = 6.0;
        let spacing = 4.0;
        let mut cur_x = self.main_rect.x + padding;

        for (idx, &item) in Self::MAIN_ITEMS.iter().enumerate() {
            if idx == 6 || idx == 8 {
                cur_x += 8.0;
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
                | ToolbarItem::ToolArrow
                | ToolbarItem::ToolPen
                | ToolbarItem::ToolMarker
                | ToolbarItem::ToolMosaic
                | ToolbarItem::ToolText => {
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

    /// 处理二级属性栏内的粗细与颜色点击
    fn handle_sub_click(&mut self, px: f32, py: f32, sub_rect: Rect) {
        let padding = 8.0;
        let mut cur_x = sub_rect.x + padding;
        let center_y = sub_rect.y + sub_rect.height / 2.0;

        for level in [
            StrokeWidthLevel::Thin,
            StrokeWidthLevel::Medium,
            StrokeWidthLevel::Thick,
        ] {
            let hit_box = Rect {
                x: cur_x - 6.0,
                y: center_y - 12.0,
                width: 24.0,
                height: 24.0,
            };
            if hit_box.contains(px, py) {
                self.sub_options.stroke_width = level;
                return;
            }
            cur_x += 24.0;
        }

        cur_x += 16.0;

        for color in preset_colors() {
            let hit_box = Rect {
                x: cur_x - 4.0,
                y: center_y - 12.0,
                width: 24.0,
                height: 24.0,
            };
            if hit_box.contains(px, py) {
                self.sub_options.color = color;
                return;
            }
            cur_x += 26.0;
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
}
