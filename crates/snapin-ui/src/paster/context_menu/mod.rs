//! 贴图右键上下文菜单状态机与鼠标交互调度。

pub mod items;
pub mod render;

pub use items::*;

use crate::geometry::Point;

/// 菜单行命中的列表、纵坐标与首行起点。
#[derive(Clone, Copy)]
struct MenuRowQuery<'a> {
    items: &'a [MenuItem],
    y: f32,
    start_y: f32,
}

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

/// 打开上下文菜单所需的锚点、窗口边界与菜单项。
pub struct OpenMenuRequest<'a> {
    pub x: f32,
    pub y: f32,
    pub window_width: f32,
    pub window_height: f32,
    pub main_items: &'a [MenuItem],
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
    pub fn open_at(&mut self, request: OpenMenuRequest<'_>) {
        let OpenMenuRequest {
            x,
            y,
            window_width: win_w,
            window_height: win_h,
            main_items,
        } = request;
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
        pointer: Point,
        items: &[MenuItem],
        smooth_scaling: bool,
    ) -> bool {
        if !self.is_open {
            return false;
        }
        if let Some(changed) = self.handle_submenu_hover(pointer, smooth_scaling) {
            return changed;
        }
        self.handle_main_hover(pointer, items, smooth_scaling)
    }

    /// 处理二级菜单内部悬停；返回 `None` 表示指针不在当前二级菜单内。
    fn handle_submenu_hover(&mut self, pointer: Point, smooth_scaling: bool) -> Option<bool> {
        let (sub_type, sub_x, sub_y) = self.active_submenu?;
        let scale = self.ui_scale;
        let sub_items = Self::build_sub_items(sub_type, smooth_scaling);
        let sub_height = Self::items_total_height(&sub_items, self.item_height, scale);
        let sub_width = 160.0 * scale;
        if pointer.x < sub_x
            || pointer.x > sub_x + sub_width
            || pointer.y < sub_y
            || pointer.y > sub_y + sub_height
        {
            return None;
        }

        let found = self
            .row_hit(&sub_items, pointer.y, sub_y + 5.0 * scale)
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
        pointer: Point,
        items: &[MenuItem],
        smooth_scaling: bool,
    ) -> bool {
        let scale = self.ui_scale;
        let main_height = Self::items_total_height(items, self.item_height, scale);
        if pointer.x < self.x
            || pointer.x > self.x + self.width
            || pointer.y < self.y
            || pointer.y > self.y + main_height
        {
            return false;
        }
        let Some(row) = self.row_hit(items, pointer.y, self.y + 5.0 * scale) else {
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
        pointer: Point,
        main_items: &[MenuItem],
        smooth_scaling: bool,
    ) -> Option<MenuItemId> {
        if !self.is_open {
            return None;
        }

        if let Some(id) = self.submenu_click(pointer, smooth_scaling) {
            self.close();
            return Some(id);
        }
        if let Some(id) = self.main_click(pointer, main_items) {
            self.close();
            return Some(id);
        }

        // 点击在菜单外部或不可执行行，关闭菜单。
        self.close();
        None
    }

    /// 检测二级子菜单点击。
    fn submenu_click(&self, pointer: Point, smooth_scaling: bool) -> Option<MenuItemId> {
        let (sub_type, sub_x, sub_y) = self.active_submenu?;
        let scale = self.ui_scale;
        let sub_items = Self::build_sub_items(sub_type, smooth_scaling);
        let sub_height = Self::items_total_height(&sub_items, self.item_height, scale);
        let sub_width = 160.0 * scale;
        if pointer.x < sub_x
            || pointer.x > sub_x + sub_width
            || pointer.y < sub_y
            || pointer.y > sub_y + sub_height
        {
            return None;
        }
        self.row_id(&sub_items, pointer.y, sub_y + 5.0 * scale)
    }

    /// 检测主菜单可执行行点击；带子菜单的行只负责展开，不直接触发动作。
    fn main_click(&self, pointer: Point, main_items: &[MenuItem]) -> Option<MenuItemId> {
        let scale = self.ui_scale;
        let main_height = Self::items_total_height(main_items, self.item_height, scale);
        if pointer.x < self.x
            || pointer.x > self.x + self.width
            || pointer.y < self.y
            || pointer.y > self.y + main_height
        {
            return None;
        }
        self.row_id_filtered(
            MenuRowQuery {
                items: main_items,
                y: pointer.y,
                start_y: self.y + 5.0 * scale,
            },
            |item| item.submenu.is_none(),
        )
    }

    /// 返回指定纵坐标命中的菜单项 ID。
    fn row_id(&self, items: &[MenuItem], y: f32, start_y: f32) -> Option<MenuItemId> {
        self.row_id_filtered(MenuRowQuery { items, y, start_y }, |_| true)
    }

    /// 返回指定纵坐标且满足过滤条件的菜单项 ID。
    fn row_id_filtered(
        &self,
        query: MenuRowQuery<'_>,
        predicate: impl Fn(&MenuItem) -> bool,
    ) -> Option<MenuItemId> {
        let separator_height = 8.0 * self.ui_scale;
        let mut top = query.start_y;
        for item in query.items {
            let height = if item.is_separator {
                separator_height
            } else {
                self.item_height
            };
            if query.y >= top && query.y < top + height && !item.is_separator && predicate(item) {
                return item.id;
            }
            top += height;
        }
        None
    }
}

#[cfg(test)]
mod tests;
