//! 贴图右键上下文菜单项定义与工厂构造函数。

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
    /// 创建分割线菜单项
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

    /// 创建常规可点击动作菜单项
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

    /// 创建带勾选框状态的菜单项
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

    /// 创建展开二级子菜单的父级项
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
}
