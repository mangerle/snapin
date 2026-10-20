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
    pub fn required_menu_extent(&self, x: f32, y: f32, main_items: &[MenuItem]) -> (f32, f32) {
        let s = self.ui_scale;
        let total_h = Self::items_total_height(main_items, self.item_height, s);

        // 包含主菜单与二级子菜单宽度的总预估（主菜单 170*s + 二级菜单 160*s）
        let max_w = (170.0 + 160.0) * s;
        let req_w = x + max_w + 10.0 * s;
        let req_h = y + total_h + 10.0 * s;
        (req_w, req_h)
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

        // 1. 检查是否悬停在二级菜单区域
        let s = self.ui_scale;
        let sep_h = 8.0 * s;
        let pad_v = 5.0 * s;
        let sub_w = 160.0 * s;
        let overlap = 4.0 * s;

        if let Some((sub_type, sx, sy)) = self.active_submenu {
            let sub_items = Self::build_sub_items(sub_type, smooth_scaling);
            let sub_h = Self::items_total_height(&sub_items, self.item_height, s);
            if cx >= sx && cx <= sx + sub_w && cy >= sy && cy <= sy + sub_h {
                let mut cur_y = sy + pad_v;
                let mut found_sub = None;
                for (idx, it) in sub_items.iter().enumerate() {
                    let h = if it.is_separator {
                        sep_h
                    } else {
                        self.item_height
                    };
                    if cy >= cur_y && cy < cur_y + h && !it.is_separator {
                        found_sub = Some(idx);
                        break;
                    }
                    cur_y += h;
                }
                if self.hovered_sub_idx != found_sub {
                    self.hovered_sub_idx = found_sub;
                    return true;
                }
                return false;
            }
        }

        // 2. 检查主菜单区域
        let main_h = Self::items_total_height(items, self.item_height, s);
        if cx >= self.x && cx <= self.x + self.width && cy >= self.y && cy <= self.y + main_h {
            let mut cur_y = self.y + pad_v;
            let mut found_main = None;
            for (idx, it) in items.iter().enumerate() {
                let h = if it.is_separator {
                    sep_h
                } else {
                    self.item_height
                };
                if cy >= cur_y && cy < cur_y + h && !it.is_separator {
                    found_main = Some((idx, cur_y, it.submenu));
                    break;
                }
                cur_y += h;
            }

            if let Some((idx, item_top, sub)) = found_main {
                let mut changed = false;
                if self.hovered_main_idx != Some(idx) {
                    self.hovered_main_idx = Some(idx);
                    changed = true;
                }
                if let Some(sub_type) = sub {
                    let sub_items = Self::build_sub_items(sub_type, smooth_scaling);
                    let sub_h = Self::items_total_height(&sub_items, self.item_height, s);

                    let sub_x = if self.x + self.width + sub_w - overlap > self.win_w
                        && self.x >= sub_w - overlap
                    {
                        self.x - sub_w + overlap
                    } else {
                        self.x + self.width - overlap
                    };

                    let sub_y = if item_top + sub_h > self.win_h {
                        (self.win_h - sub_h).max(0.0)
                    } else {
                        item_top - overlap
                    };

                    let next_sub = Some((sub_type, sub_x.max(0.0), sub_y.max(0.0)));
                    if self.active_submenu != next_sub {
                        self.active_submenu = next_sub;
                        self.hovered_sub_idx = None;
                        changed = true;
                    }
                } else if self.active_submenu.is_some() {
                    self.active_submenu = None;
                    self.hovered_sub_idx = None;
                    changed = true;
                }
                return changed;
            }
        }

        false
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
