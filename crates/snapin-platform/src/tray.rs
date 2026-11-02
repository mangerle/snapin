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
    /// 智能打码
    SmartRedact,
    /// 二维码识别
    Qr,
    /// 美化导出
    Beautify,
    /// 打开贴图工作区面板
    Workspace,
    /// 打开剪贴板历史面板
    ClipboardHistory,
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

/// 托盘菜单的单一描述源，Linux SNI 与 Windows/macOS 原生菜单共用。
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayMenuEntry {
    Action {
        label: &'static str,
        action: TrayAction,
    },
    Separator,
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn tray_menu() -> Vec<TrayMenuEntry> {
    vec![
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("截屏"),
            action: TrayAction::Snip,
        },
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("全屏截图"),
            action: TrayAction::Fullscreen,
        },
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("取色器"),
            action: TrayAction::PickColor,
        },
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("贴图"),
            action: TrayAction::Paste,
        },
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("智能打码"),
            action: TrayAction::SmartRedact,
        },
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("二维码"),
            action: TrayAction::Qr,
        },
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("美化导出"),
            action: TrayAction::Beautify,
        },
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("贴图工作区"),
            action: TrayAction::Workspace,
        },
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("剪贴板历史"),
            action: TrayAction::ClipboardHistory,
        },
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("隐藏/显示所有贴图"),
            action: TrayAction::TogglePasters,
        },
        TrayMenuEntry::Separator,
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("首选项..."),
            action: TrayAction::Preferences,
        },
        TrayMenuEntry::Action {
            label: snapin_foundation::i18n::tr("退出 Snapin"),
            action: TrayAction::Exit,
        },
    ]
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native {
    use super::{tray_menu, TrayAction, TrayMenuEntry};
    use crate::error::PlatformError;
    use std::collections::HashMap;
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
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
            let mut action_map = HashMap::with_capacity(13);
            for entry in tray_menu() {
                match entry {
                    TrayMenuEntry::Action { label, action } => {
                        Self::append_item(&menu, label, action, &mut action_map)?;
                    }
                    TrayMenuEntry::Separator => {
                        let separator = PredefinedMenuItem::separator();
                        menu.append(&separator)
                            .map_err(|error| PlatformError::Tray {
                                message: format!("向托盘菜单追加分隔线失败: {error}"),
                            })?;
                    }
                }
            }

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

#[cfg(target_os = "linux")]
mod linux {
    use super::{tray_menu, TrayAction, TrayMenuEntry};
    use crate::error::PlatformError;
    use ksni::{blocking::TrayMethods, menu::*, Tray};
    use tokio::sync::mpsc::{channel, Receiver, Sender};

    /// 托盘动作队列容量；托盘点击是低频人工操作，满队列时直接丢弃最旧语义之外的
    /// 新动作并记录警告，绝不使用无界通道兜底。
    const TRAY_ACTION_QUEUE_CAPACITY: usize = 32;

    struct LinuxSniTray {
        action_tx: Sender<TrayAction>,
        icon_data: Option<ksni::Icon>,
    }

    impl Tray for LinuxSniTray {
        fn id(&self) -> String {
            "snapin".into()
        }

        fn title(&self) -> String {
            "Snapin 截图与贴图工具".into()
        }

        fn icon_name(&self) -> String {
            "snapin".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon_data.clone().into_iter().collect()
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            if self.action_tx.try_send(TrayAction::Snip).is_err() {
                tracing::warn!("托盘动作队列已满，忽略本次托盘激活");
            }
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            let entries = tray_menu();
            entries
                .into_iter()
                .map(|entry| match entry {
                    TrayMenuEntry::Separator => MenuItem::Separator,
                    TrayMenuEntry::Action { label, action } => StandardItem {
                        label: label.into(),
                        activate: Box::new(move |this: &mut Self| this.emit(action)),
                        ..Default::default()
                    }
                    .into(),
                })
                .collect()
        }
    }

    impl LinuxSniTray {
        fn emit(&self, action: TrayAction) {
            if self.action_tx.try_send(action).is_err() {
                tracing::warn!("托盘动作队列已满，忽略本次菜单动作: {action:?}");
            }
        }
    }

    /// Linux 平台原生 StatusNotifierItem 托盘管理器
    pub struct LinuxPlatformTray {
        _handle: ksni::blocking::Handle<LinuxSniTray>,
        action_rx: Receiver<TrayAction>,
    }

    impl LinuxPlatformTray {
        pub fn init(rgba_icon: Option<(Vec<u8>, u32, u32)>) -> Result<Self, PlatformError> {
            let (tx, rx) = channel(TRAY_ACTION_QUEUE_CAPACITY);

            let icon_data = rgba_icon.map(|(rgba, width, height)| {
                let mut argb = Vec::with_capacity(rgba.len());
                let (chunks, _) = rgba.as_chunks::<4>();
                for &[r, g, b, a] in chunks {
                    argb.push(a);
                    argb.push(r);
                    argb.push(g);
                    argb.push(b);
                }
                ksni::Icon {
                    width: width as i32,
                    height: height as i32,
                    data: argb,
                }
            });

            let tray = LinuxSniTray {
                action_tx: tx,
                icon_data,
            };

            let handle = tray.spawn().map_err(|e| PlatformError::Tray {
                message: format!("启动 Linux StatusNotifierItem (SNI) 系统托盘服务失败: {e}"),
            })?;

            Ok(Self {
                _handle: handle,
                action_rx: rx,
            })
        }

        pub fn try_recv(&mut self) -> Option<TrayAction> {
            self.action_rx.try_recv().ok()
        }
    }
}

/// 统一跨平台托盘对象封装
pub struct PlatformTray {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    inner: native::NativePlatformTray,
    #[cfg(target_os = "linux")]
    inner: linux::LinuxPlatformTray,
}

impl PlatformTray {
    /// 初始化跨平台系统托盘
    ///
    /// - Linux: 创建基于 StatusNotifierItem (SNI) 的系统原生托盘，即开即用免注销重登；
    /// - Windows / macOS: 基于原生系统托盘 API 提供上下文菜单与动作回调。
    ///
    /// # Errors
    ///
    /// 系统托盘后端初始化失败时返回 [`PlatformError`]。
    pub fn new(_rgba_icon: Option<(Vec<u8>, u32, u32)>) -> Result<Self, PlatformError> {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let (pixels, width, height) = _rgba_icon.ok_or_else(|| PlatformError::Tray {
                message: "Windows 与 macOS 初始化托盘必须提供图标 RGBA 像素数据".to_string(),
            })?;
            let inner = native::NativePlatformTray::init((pixels, width, height))?;
            Ok(Self { inner })
        }

        #[cfg(target_os = "linux")]
        {
            let inner = linux::LinuxPlatformTray::init(_rgba_icon)?;
            Ok(Self { inner })
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Ok(Self {})
        }
    }

    /// 非阻塞检查是否触发了托盘指令
    pub fn poll_action(&mut self) -> Option<TrayAction> {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            self.inner.try_recv()
        }

        #[cfg(target_os = "linux")]
        {
            self.inner.try_recv()
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 托盘菜单必须保持动作唯一，避免同一个菜单项触发两次或遗漏平台扩展动作。
    #[test]
    fn test_tray_menu_actions_are_unique_and_non_empty() {
        let mut actions = Vec::new();
        for entry in tray_menu() {
            match entry {
                TrayMenuEntry::Action { label, action } => {
                    assert!(!label.is_empty(), "托盘菜单标签不能为空");
                    assert!(!actions.contains(&action), "托盘动作重复: {action:?}");
                    actions.push(action);
                }
                TrayMenuEntry::Separator => {}
            }
        }
        assert_eq!(actions.len(), 12, "托盘动作数量应与统一菜单描述一致");
    }
}
