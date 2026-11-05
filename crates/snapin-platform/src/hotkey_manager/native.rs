//! Windows/macOS 原生全局热键监听。

use super::{configured_bindings, hotkey_to_accelerator, HotkeyAction};
use crate::error::PlatformError;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use snapin_config::config::HotkeyConfig;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct NativeHotkeyListener {
    _manager: GlobalHotKeyManager,
}

impl NativeHotkeyListener {
    pub(crate) fn init(
        config: &HotkeyConfig,
        on_action: Arc<dyn Fn(HotkeyAction) + Send + Sync>,
    ) -> Result<Self, PlatformError> {
        let manager = GlobalHotKeyManager::new().map_err(|e| PlatformError::Hotkey {
            message: format!("初始化原生全局热键管理器失败: {e}"),
        })?;

        let bindings = configured_bindings(config);
        let mut action_map = HashMap::with_capacity(bindings.len());

        for binding in bindings {
            if binding.hotkey.is_disabled() {
                continue;
            }
            let acc = hotkey_to_accelerator(binding.hotkey);
            match acc.parse::<HotKey>() {
                Ok(hk) => {
                    if let Err(e) = manager.register(hk) {
                        tracing::warn!("注册快捷键 [{acc}] 失败: {e}");
                    } else {
                        action_map.insert(hk.id, binding.action);
                    }
                }
                Err(error) => {
                    tracing::warn!("快捷键 [{acc}] 无法转换为原生热键，已跳过: {error}");
                }
            }
        }

        // 事件回调是 global-hotkey 的进程级唯一入口；只把命中的动作交给宿主，
        // 由宿主投递到 GUI 事件循环，避免固定周期轮询全局事件通道。
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state == HotKeyState::Pressed {
                if let Some(action) = action_map.get(&event.id) {
                    on_action(*action);
                }
            }
        }));

        Ok(Self { _manager: manager })
    }
}

impl Drop for NativeHotkeyListener {
    fn drop(&mut self) {
        // 释放进程级回调，避免监听器销毁后仍向已关闭的工作区投递热键动作。
        GlobalHotKeyEvent::set_event_handler::<fn(GlobalHotKeyEvent)>(None);
    }
}
