//! Windows/macOS 原生全局热键监听。

use super::{configured_bindings, hotkey_to_accelerator, HotkeyAction};
use crate::error::PlatformError;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use snapin_core::config::HotkeyConfig;
use std::collections::HashMap;

pub(crate) struct NativeHotkeyListener {
    _manager: GlobalHotKeyManager,
    action_map: HashMap<u32, HotkeyAction>,
}

impl NativeHotkeyListener {
    pub(crate) fn init(config: &HotkeyConfig) -> Result<Self, PlatformError> {
        let manager = GlobalHotKeyManager::new().map_err(|e| PlatformError::Hotkey {
            message: format!("初始化原生全局热键管理器失败: {e}"),
        })?;

        let bindings = configured_bindings(config);
        let mut action_map = HashMap::with_capacity(bindings.len());

        for binding in bindings {
            let acc = hotkey_to_accelerator(binding.hotkey);
            if let Ok(hk) = acc.parse::<HotKey>() {
                if let Err(e) = manager.register(hk) {
                    tracing::warn!("注册快捷键 [{acc}] 失败: {e}");
                } else {
                    action_map.insert(hk.id, binding.action);
                }
            }
        }

        Ok(Self {
            _manager: manager,
            action_map,
        })
    }

    pub(crate) fn try_recv(&self) -> Option<HotkeyAction> {
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed {
                if let Some(&action) = self.action_map.get(&event.id) {
                    return Some(action);
                }
            }
        }
        None
    }
}
