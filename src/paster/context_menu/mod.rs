//! 贴图右键上下文菜单数据模型与命中调度。

pub mod render;

/// 菜单项动作标识
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuItemId {
    CopyImage,
    SaveImageAs,
    ToggleToolbar,
    ToggleShadow,
    Scale(f32),
    ToggleSmoothScaling,
    RotateLeft,
    RotateRight,
    FlipHorizontal,
    FlipVertical,
    PasteFromClipboard,
    OpenInFolder,
    Close,
    Destroy,
}

/// 二级子菜单类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubMenuType {
    Scale,
    ImageProcessing,
}

/// 单个菜单行数据描述
#[derive(Debug, Clone)]
pub struct MenuItem {
    pub id: Option<MenuItemId>,
    pub label: &'static str,
    pub is_separator: bool,
    pub is_checkbox: bool,
    pub is_checked: bool,
    pub submenu: Option<SubMenuType>,
}

impl MenuItem {
    pub fn separator() -> Self {
        Self {
            id: None,
            label: "",
            is_separator: true,
            is_checkbox: false,
            is_checked: false,
            submenu: None,
        }
    }

    pub fn action(id: MenuItemId, label: &'static str) -> Self {
        Self {
            id: Some(id),
            label,
            is_separator: false,
            is_checkbox: false,
            is_checked: false,
            submenu: None,
        }
    }

    pub fn checkbox(id: MenuItemId, label: &'static str, checked: bool) -> Self {
        Self {
            id: Some(id),
            label,
            is_separator: false,
            is_checkbox: true,
            is_checked: checked,
            submenu: None,
        }
    }

    pub fn parent(label: &'static str, submenu: SubMenuType) -> Self {
        Self {
            id: None,
            label,
            is_separator: false,
            is_checkbox: false,
            is_checked: false,
            submenu: Some(submenu),
        }
    }
}

/// 右键上下文菜单运行时状态
pub struct ContextMenuState {
    pub is_open: bool,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub item_height: f32,
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
            hovered_main_idx: None,
            active_submenu: None,
            hovered_sub_idx: None,
        }
    }
}

impl ContextMenuState {
    /// 在指定坐标弹出上下文菜单
    pub fn open_at(&mut self, x: f32, y: f32) {
        self.is_open = true;
        self.x = x.max(0.0);
        self.y = y.max(0.0);
        self.hovered_main_idx = None;
        self.active_submenu = None;
        self.hovered_sub_idx = None;
    }

    /// 关闭上下文菜单
    pub fn close(&mut self) {
        self.is_open = false;
        self.hovered_main_idx = None;
        self.active_submenu = None;
        self.hovered_sub_idx = None;
    }

    /// 构建当前的主菜单项列表
    pub fn build_main_items(show_toolbar: bool, has_shadow: bool) -> Vec<MenuItem> {
        vec![
            MenuItem::action(MenuItemId::CopyImage, "复制图像"),
            MenuItem::action(MenuItemId::SaveImageAs, "图像另存为..."),
            MenuItem::checkbox(MenuItemId::ToggleToolbar, "显示工具条", show_toolbar),
            MenuItem::checkbox(MenuItemId::ToggleShadow, "窗口阴影", has_shadow),
            MenuItem::separator(),
            MenuItem::parent("缩放", SubMenuType::Scale),
            MenuItem::parent("图像处理", SubMenuType::ImageProcessing),
            MenuItem::action(MenuItemId::PasteFromClipboard, "粘贴"),
            MenuItem::action(MenuItemId::OpenInFolder, "在文件夹中查看"),
            MenuItem::separator(),
            MenuItem::action(MenuItemId::Close, "关闭"),
            MenuItem::action(MenuItemId::Destroy, "销毁"),
        ]
    }

    /// 构建指定的二级子菜单项列表
    pub fn build_sub_items(sub_type: SubMenuType, smooth_scaling: bool) -> Vec<MenuItem> {
        match sub_type {
            SubMenuType::Scale => vec![
                MenuItem::action(MenuItemId::Scale(0.3), "30%"),
                MenuItem::action(MenuItemId::Scale(0.5), "50%"),
                MenuItem::action(MenuItemId::Scale(1.0), "100%"),
                MenuItem::action(MenuItemId::Scale(2.0), "200%"),
                MenuItem::separator(),
                MenuItem::checkbox(
                    MenuItemId::ToggleSmoothScaling,
                    "是否平滑处理？",
                    smooth_scaling,
                ),
            ],
            SubMenuType::ImageProcessing => vec![
                MenuItem::action(MenuItemId::RotateLeft, "向左旋转 (90°)"),
                MenuItem::action(MenuItemId::RotateRight, "向右旋转 (90°)"),
                MenuItem::action(MenuItemId::FlipHorizontal, "水平翻转"),
                MenuItem::action(MenuItemId::FlipVertical, "垂直翻转"),
            ],
        }
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
        if let Some((sub_type, sx, sy)) = self.active_submenu {
            let sub_items = Self::build_sub_items(sub_type, smooth_scaling);
            let sub_w = 160.0;
            let sub_h = sub_items
                .iter()
                .map(|it| {
                    if it.is_separator {
                        8.0
                    } else {
                        self.item_height
                    }
                })
                .sum::<f32>()
                + 10.0;
            if cx >= sx && cx <= sx + sub_w && cy >= sy && cy <= sy + sub_h {
                let mut cur_y = sy + 5.0;
                let mut found_sub = None;
                for (idx, it) in sub_items.iter().enumerate() {
                    let h = if it.is_separator {
                        8.0
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
        let main_h = items
            .iter()
            .map(|it| {
                if it.is_separator {
                    8.0
                } else {
                    self.item_height
                }
            })
            .sum::<f32>()
            + 10.0;
        if cx >= self.x && cx <= self.x + self.width && cy >= self.y && cy <= self.y + main_h {
            let mut cur_y = self.y + 5.0;
            let mut found_main = None;
            for (idx, it) in items.iter().enumerate() {
                let h = if it.is_separator {
                    8.0
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
                    let next_sub = Some((sub_type, self.x + self.width - 4.0, item_top - 4.0));
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

        // 1. 检测二级子菜单点击
        if let Some((sub_type, sx, sy)) = self.active_submenu {
            let sub_items = Self::build_sub_items(sub_type, smooth_scaling);
            let sub_w = 160.0;
            let sub_h = sub_items
                .iter()
                .map(|it| {
                    if it.is_separator {
                        8.0
                    } else {
                        self.item_height
                    }
                })
                .sum::<f32>()
                + 10.0;
            if cx >= sx && cx <= sx + sub_w && cy >= sy && cy <= sy + sub_h {
                let mut cur_y = sy + 5.0;
                for it in sub_items.iter() {
                    let h = if it.is_separator {
                        8.0
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
        let main_h = main_items
            .iter()
            .map(|it| {
                if it.is_separator {
                    8.0
                } else {
                    self.item_height
                }
            })
            .sum::<f32>()
            + 10.0;
        if cx >= self.x && cx <= self.x + self.width && cy >= self.y && cy <= self.y + main_h {
            let mut cur_y = self.y + 5.0;
            for it in main_items.iter() {
                let h = if it.is_separator {
                    8.0
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
mod tests {
    use super::*;

    #[test]
    fn test_context_menu_open_and_close() {
        let mut menu = ContextMenuState::default();
        assert!(!menu.is_open);
        menu.open_at(100.0, 150.0);
        assert!(menu.is_open);
        assert_eq!(menu.x, 100.0);
        assert_eq!(menu.y, 150.0);
        menu.close();
        assert!(!menu.is_open);
    }

    #[test]
    fn test_context_menu_items_structure() {
        let items = ContextMenuState::build_main_items(true, false);
        assert!(items.len() >= 10);
        let sub_scale = ContextMenuState::build_sub_items(SubMenuType::Scale, true);
        assert_eq!(sub_scale.len(), 6);
        let sub_img = ContextMenuState::build_sub_items(SubMenuType::ImageProcessing, true);
        assert_eq!(sub_img.len(), 4);
    }

    #[test]
    fn test_context_menu_click_action() {
        let mut menu = ContextMenuState::default();
        menu.open_at(50.0, 50.0);
        let items = ContextMenuState::build_main_items(false, true);

        // 点击第一项 "复制图像" (y 在 55.0 ~ 83.0 之间)
        let clicked = menu.handle_click(60.0, 65.0, &items, true);
        assert_eq!(clicked, Some(MenuItemId::CopyImage));
        assert!(!menu.is_open);
    }
}
