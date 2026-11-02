//! 贴图右键上下文菜单状态机与鼠标交互调度。

pub mod items;
pub mod render;

pub use items::*;

/// 右键上下文菜单运行时状态
pub struct ContextMenuState {
    pub is_open: bool,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub item_height: f32,
    pub ui_scale: f32,
    pub win_w: f32,
    pub win_h: f32,
    pub hovered_main_idx: Option<usize>,
    pub active_submenu: Option<(SubMenuType, f32, f32)>,
    pub hovered_sub_idx: Option<usize>,
}

/// 主菜单中当前命中的非分隔行。
#[derive(Debug, Clone, Copy)]
struct MenuRowHit {
    index: usize,
    top: f32,
    submenu: Option<SubMenuType>,
}

impl Default for ContextMenuState {
    fn default() -> Self {
        Self {
            is_open: false,
            x: 0.0,
            y: 0.0,
            width: 170.0,
            item_height: 28.0,
            ui_scale: 1.0,
            win_w: 1920.0,
            win_h: 1080.0,
            hovered_main_idx: None,
            active_submenu: None,
            hovered_sub_idx: None,
        }
    }
}

impl ContextMenuState {
    /// 设置菜单缩放倍率并重新计算布局基准尺寸
    pub fn set_ui_scale(&mut self, scale: f32) {
        let s = scale.max(0.5);
        self.ui_scale = s;
        self.width = 170.0 * s;
        self.item_height = 28.0 * s;
    }

    /// 计算完全展开上下文菜单（含可能的二级子菜单）所需的最小安全包围尺寸
    /// 菜单完整展开所需的最小窗口尺寸（绝对占位，与光标位置无关）。
    ///
    /// 旧实现按"光标坐标 + 菜单宽高"返回，完全忽略了 `open_at` 会向左/上翻转避让，
    /// 导致贴图明明比菜单大得多、只要在右下角右键就把窗口无谓撑大；
    /// 撑出的空区域既增加重绘与合成开销，也会拦截本应落到桌面的鼠标事件
    /// （透明像素同样参与 Wayland 表面输入命中）。
    pub fn required_menu_extent(&self, main_items: &[MenuItem]) -> (f32, f32) {
        let s = self.ui_scale;
        let total_h = Self::items_total_height(main_items, self.item_height, s);

        // 主菜单 170*s + 二级子菜单 160*s，两侧各留 10*s 投影余量
        let max_w = (170.0 + 160.0) * s + 20.0 * s;
        (max_w, total_h + 20.0 * s)
    }

    /// 在指定坐标弹出上下文菜单，并自适应窗口物理边界进行上下左右智能避让
    pub fn open_at(&mut self, x: f32, y: f32, win_w: f32, win_h: f32, main_items: &[MenuItem]) {
        self.is_open = true;
        self.win_w = win_w;
        self.win_h = win_h;
        self.hovered_main_idx = None;
        self.active_submenu = None;
        self.hovered_sub_idx = None;

        let s = self.ui_scale;
        let total_h = Self::items_total_height(main_items, self.item_height, s);

        let target_y = if y + total_h > win_h {
            if y >= total_h {
                y - total_h
            } else {
                (win_h - total_h).max(0.0)
            }
        } else {
            y
        };

        let target_x = if x + self.width > win_w {
            if x >= self.width {
                x - self.width
            } else {
                (win_w - self.width).max(0.0)
            }
        } else {
            x
        };

        self.x = target_x.max(0.0);
        self.y = target_y.max(0.0);
    }

    /// 关闭上下文菜单
    pub fn close(&mut self) {
        self.is_open = false;
        self.hovered_main_idx = None;
        self.active_submenu = None;
        self.hovered_sub_idx = None;
    }

    /// 构建当前的主菜单项列表（委托至 MenuItem）
    pub fn build_main_items(show_toolbar: bool, has_shadow: bool) -> Vec<MenuItem> {
        MenuItem::build_main_items(show_toolbar, has_shadow)
    }

    /// 构建指定的二级子菜单项列表（委托至 MenuItem）
    pub fn build_sub_items(sub_type: SubMenuType, smooth_scaling: bool) -> Vec<MenuItem> {
        MenuItem::build_sub_items(sub_type, smooth_scaling)
    }

    /// 计算菜单项列表的总高度（含上下内边距）
    pub(crate) fn items_total_height(items: &[MenuItem], item_height: f32, scale: f32) -> f32 {
        let separator_height = 8.0 * scale;
        items
            .iter()
            .map(|item| {
                if item.is_separator {
                    separator_height
                } else {
                    item_height
                }
            })
            .sum::<f32>()
            + 10.0 * scale
    }

    /// 鼠标移动检测悬停与二级菜单触发
    pub fn handle_mouse_move(
        &mut self,
        cx: f32,
        cy: f32,
        items: &[MenuItem],
        smooth_scaling: bool,
    ) -> bool {
        if !self.is_open {
            return false;
        }
        if let Some(changed) = self.handle_submenu_hover(cx, cy, smooth_scaling) {
            return changed;
        }
        self.handle_main_hover(cx, cy, items, smooth_scaling)
    }

    /// 处理二级菜单内部悬停；返回 `None` 表示指针不在当前二级菜单内。
    fn handle_submenu_hover(&mut self, cx: f32, cy: f32, smooth_scaling: bool) -> Option<bool> {
        let (sub_type, sub_x, sub_y) = self.active_submenu?;
        let scale = self.ui_scale;
        let sub_items = Self::build_sub_items(sub_type, smooth_scaling);
        let sub_height = Self::items_total_height(&sub_items, self.item_height, scale);
        let sub_width = 160.0 * scale;
        if cx < sub_x || cx > sub_x + sub_width || cy < sub_y || cy > sub_y + sub_height {
            return None;
        }

        let found = self
            .row_hit(&sub_items, cy, sub_y + 5.0 * scale)
            .map(|row| row.index);
        if self.hovered_sub_idx == found {
            Some(false)
        } else {
            self.hovered_sub_idx = found;
            Some(true)
        }
    }

    /// 处理主菜单悬停并按需打开或关闭二级菜单。
    fn handle_main_hover(
        &mut self,
        cx: f32,
        cy: f32,
        items: &[MenuItem],
        smooth_scaling: bool,
    ) -> bool {
        let scale = self.ui_scale;
        let main_height = Self::items_total_height(items, self.item_height, scale);
        if cx < self.x || cx > self.x + self.width || cy < self.y || cy > self.y + main_height {
            return false;
        }
        let Some(row) = self.row_hit(items, cy, self.y + 5.0 * scale) else {
            return false;
        };
        let mut changed = false;
        if self.hovered_main_idx != Some(row.index) {
            self.hovered_main_idx = Some(row.index);
            changed = true;
        }
        changed |= self.update_submenu(row, smooth_scaling);
        changed
    }

    /// 根据当前主菜单行同步二级菜单位置；无子菜单时关闭旧菜单。
    fn update_submenu(&mut self, row: MenuRowHit, smooth_scaling: bool) -> bool {
        let Some(sub_type) = row.submenu else {
            return self.close_submenu();
        };
        let scale = self.ui_scale;
        let sub_items = Self::build_sub_items(sub_type, smooth_scaling);
        let sub_height = Self::items_total_height(&sub_items, self.item_height, scale);
        let (sub_x, sub_y) = self.submenu_origin(row.top, sub_height, scale);
        let next = Some((sub_type, sub_x, sub_y));
        if self.active_submenu == next {
            false
        } else {
            self.active_submenu = next;
            self.hovered_sub_idx = None;
            true
        }
    }

    /// 计算二级菜单在窗口边界内的水平翻转与垂直避让位置。
    fn submenu_origin(&self, item_top: f32, sub_height: f32, scale: f32) -> (f32, f32) {
        let sub_width = 160.0 * scale;
        let overlap = 4.0 * scale;
        let sub_x = if self.x + self.width + sub_width - overlap > self.win_w
            && self.x >= sub_width - overlap
        {
            self.x - sub_width + overlap
        } else {
            self.x + self.width - overlap
        };
        let sub_y = if item_top + sub_height > self.win_h {
            (self.win_h - sub_height).max(0.0)
        } else {
            item_top - overlap
        };
        (sub_x.max(0.0), sub_y.max(0.0))
    }

    /// 关闭二级菜单并清除其悬停状态。
    fn close_submenu(&mut self) -> bool {
        if self.active_submenu.is_none() {
            return false;
        }
        self.active_submenu = None;
        self.hovered_sub_idx = None;
        true
    }

    /// 在菜单项列表中查找当前纵坐标命中的非分隔行。
    fn row_hit(&self, items: &[MenuItem], cy: f32, start_y: f32) -> Option<MenuRowHit> {
        let separator_height = 8.0 * self.ui_scale;
        let mut top = start_y;
        for (index, item) in items.iter().enumerate() {
            let height = if item.is_separator {
                separator_height
            } else {
                self.item_height
            };
            if cy >= top && cy < top + height && !item.is_separator {
                return Some(MenuRowHit {
                    index,
                    top,
                    submenu: item.submenu,
                });
            }
            top += height;
        }
        None
    }

    /// 点击菜单项判定
    pub fn handle_click(
        &mut self,
        cx: f32,
        cy: f32,
        main_items: &[MenuItem],
        smooth_scaling: bool,
    ) -> Option<MenuItemId> {
        if !self.is_open {
            return None;
        }

        let s = self.ui_scale;
        let sep_h = 8.0 * s;
        let pad_v = 5.0 * s;
        let sub_w = 160.0 * s;

        // 1. 检测二级子菜单点击
        if let Some((sub_type, sx, sy)) = self.active_submenu {
            let sub_items = Self::build_sub_items(sub_type, smooth_scaling);
            let sub_h = Self::items_total_height(&sub_items, self.item_height, s);
            if cx >= sx && cx <= sx + sub_w && cy >= sy && cy <= sy + sub_h {
                let mut cur_y = sy + pad_v;
                for it in sub_items.iter() {
                    let h = if it.is_separator {
                        sep_h
                    } else {
                        self.item_height
                    };
                    if cy >= cur_y && cy < cur_y + h && !it.is_separator {
                        self.close();
                        return it.id;
                    }
                    cur_y += h;
                }
            }
        }

        // 2. 检测主菜单点击
        let main_h = Self::items_total_height(main_items, self.item_height, s);
        if cx >= self.x && cx <= self.x + self.width && cy >= self.y && cy <= self.y + main_h {
            let mut cur_y = self.y + pad_v;
            for it in main_items.iter() {
                let h = if it.is_separator {
                    sep_h
                } else {
                    self.item_height
                };
                if cy >= cur_y && cy < cur_y + h && !it.is_separator && it.submenu.is_none() {
                    self.close();
                    return it.id;
                }
                cur_y += h;
            }
        }

        // 点击在菜单外部，关闭菜单
        self.close();
        None
    }
}

#[cfg(test)]
mod tests;
