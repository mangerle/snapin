//! 跨平台系统托盘管理抽象模块。
//!
//! 在 Windows 与 macOS 平台直接基于成熟的 tray-icon 实现原生托盘图标与上下文菜单，
//! 并提供统一的事件通道；在 Linux 平台无缝复用 GNOME 专属扩展与系统 D-Bus 服务。

use crate::error::PlatformError;

/// 托盘菜单操作指令枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// 触发交互式选区截屏
    Snip,
    /// 截取全屏
    Fullscreen,
    /// 延时截屏（单位秒）
    DelaySnip(u32),
    /// 全屏像素取色器
    PickColor,
    /// 截屏后自动复制到剪贴板
    AutoCopy,
    /// 自定义选区截屏
    CustomSnip,
    /// 从剪贴板弹出贴图窗口
    Paste,
    /// 批量显隐贴图窗口
    TogglePasters,
    /// 切换贴图工作区或分组
    SwitchGroup,
    /// 清理截屏历史缓存
    ClearHistory,
    /// 打开首选项设置窗口
    Preferences,
    /// 优雅退出应用程序
    Exit,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native {
    use super::TrayAction;
    use crate::error::PlatformError;
    use std::collections::HashMap;
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem},
        Icon, TrayIcon, TrayIconBuilder,
    };

    /// Windows / macOS 平台原生托盘管理器
    pub struct NativePlatformTray {
        _tray: TrayIcon,
        _menu: Menu,
        action_map: HashMap<tray_icon::menu::MenuId, TrayAction>,
    }

    impl NativePlatformTray {
        /// 初始化原生托盘图标与菜单
        pub fn init(rgba_icon: (Vec<u8>, u32, u32)) -> Result<Self, PlatformError> {
            let (pixels, width, height) = rgba_icon;
            let icon = Icon::from_rgba(pixels, width, height).map_err(|e| PlatformError::Tray {
                message: format!("加载托盘图标像素数据失败: {e}"),
            })?;

            let menu = Menu::new();
            let mut action_map = HashMap::with_capacity(8);

            // 构建与 Linux 扩展菜单完全对称的托盘项目
            Self::append_item(&menu, "截屏", TrayAction::Snip, &mut action_map)?;
            Self::append_item(&menu, "全屏截图", TrayAction::Fullscreen, &mut action_map)?;
            Self::append_item(&menu, "取色器", TrayAction::PickColor, &mut action_map)?;
            Self::append_item(&menu, "贴图", TrayAction::Paste, &mut action_map)?;
            Self::append_item(
                &menu,
                "隐藏/显示所有贴图",
                TrayAction::TogglePasters,
                &mut action_map,
            )?;
            Self::append_item(&menu, "首选项...", TrayAction::Preferences, &mut action_map)?;
            Self::append_item(&menu, "退出 Snapin", TrayAction::Exit, &mut action_map)?;

            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(menu.clone()))
                .with_tooltip("Snapin 截图与贴图工具")
                .with_icon(icon)
                .build()
                .map_err(|e| PlatformError::Tray {
                    message: format!("创建原生系统托盘图标失败: {e}"),
                })?;

            Ok(Self {
                _tray: tray,
                _menu: menu,
                action_map,
            })
        }

        /// 追加菜单项并记录 ID 到指令映射表
        fn append_item(
            menu: &Menu,
            text: &str,
            action: TrayAction,
            action_map: &mut HashMap<tray_icon::menu::MenuId, TrayAction>,
        ) -> Result<(), PlatformError> {
            let item = MenuItem::new(text, true, None);
            action_map.insert(item.id().clone(), action);
            menu.append(&item).map_err(|e| PlatformError::Tray {
                message: format!("向托盘菜单追加项目失败: {e}"),
            })?;
            Ok(())
        }

        /// 非阻塞拉取用户触发的托盘指令
        pub fn try_recv(&self) -> Option<TrayAction> {
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if let Some(&action) = self.action_map.get(&event.id) {
                    return Some(action);
                }
            }
            None
        }
    }
}

/// 统一跨平台托盘对象封装
pub struct PlatformTray {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    inner: native::NativePlatformTray,
}

impl PlatformTray {
    /// 初始化跨平台系统托盘
    ///
    /// - Linux: 返回无操作空对象（托盘生命周期由 GNOME 扩展或 D-Bus 闭环管理）；
    /// - Windows / macOS: 创建原生系统托盘并监听菜单动作。
    pub fn new(_rgba_icon: Option<(Vec<u8>, u32, u32)>) -> Result<Self, PlatformError> {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let (pixels, width, height) = _rgba_icon.ok_or_else(|| PlatformError::Tray {
                message: "Windows 与 macOS 初始化托盘必须提供图标 RGBA 像素数据".to_string(),
            })?;
            let inner = native::NativePlatformTray::init((pixels, width, height))?;
            Ok(Self { inner })
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Ok(Self {})
        }
    }

    /// 非阻塞检查是否触发了托盘指令
    pub fn poll_action(&self) -> Option<TrayAction> {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            self.inner.try_recv()
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            None
        }
    }
}
