//! 宿主托盘与全局热键动作的接收、排队与 GUI 线程分发。
//!
//! 平台回调可能运行在系统事件线程；这里只做有界入队与事件循环唤醒，真正的
//! 动作分发仍在 GUI 线程完成，避免跨线程操作窗口对象。

use super::actions::{handle_hotkey_action, handle_tray_action};
use crate::services::AppServices;
use snapin_config::config::Config;
use snapin_platform::{HotkeyAction, PlatformHotkeyListener, PlatformTray, TrayAction};
use snapin_ports::WorkspaceDriver;
use std::sync::Arc;
use tokio::sync::mpsc::{error::TrySendError, Receiver, Sender};

/// 宿主动作：托盘菜单与全局热键共享同一个有界队列，保持触发顺序。
#[derive(Debug)]
pub(super) enum HostAction {
    /// 托盘菜单或图标激活
    Tray(TrayAction),
    /// 系统全局快捷键
    Hotkey(HotkeyAction),
}

/// 宿主动作队列容量；托盘与热键都是低频人工操作，满队列时丢弃并告警。
pub(super) const HOST_ACTION_QUEUE_CAPACITY: usize = 32;

/// 在主线程创建原生托盘；Windows/macOS 的原生托盘必须在主线程初始化。
pub(super) fn create_daemon_tray(
    sender: &Sender<HostAction>,
    workspace: &Arc<dyn WorkspaceDriver>,
) -> Option<PlatformTray> {
    let tray_icon = image::load_from_memory(include_bytes!("../../../../assets/icons/32x32.png"))
        .ok()
        .map(|img| {
            let rgba = img.to_rgba8();
            (rgba.as_raw().clone(), rgba.width(), rgba.height())
        });
    let sender = sender.clone();
    let workspace = Arc::clone(workspace);
    PlatformTray::new(tray_icon, move |action| {
        tracing::debug!("托盘触发操作: {action:?}");
        push_host_action(&sender, workspace.as_ref(), HostAction::Tray(action));
    })
    .map_err(|error| tracing::warn!("初始化跨平台原生系统托盘失败: {error}"))
    .ok()
}

/// 在主线程注册原生全局热键监听器。
pub(super) fn create_hotkey_listener(
    cfg: &Config,
    sender: &Sender<HostAction>,
    workspace: &Arc<dyn WorkspaceDriver>,
) -> Option<PlatformHotkeyListener> {
    let sender = sender.clone();
    let workspace = Arc::clone(workspace);
    PlatformHotkeyListener::new(&cfg.hotkeys, move |action| {
        tracing::debug!("系统全局热键触发: {action:?}");
        push_host_action(&sender, workspace.as_ref(), HostAction::Hotkey(action));
    })
    .map_err(|error| tracing::warn!("初始化跨平台全局热键监听器失败: {error}"))
    .ok()
}

/// 将平台回调产生的动作写入有界队列并唤醒 GUI 线程。
///
/// 回调可能运行在平台后端线程，不能直接操作窗口；这里只做一次非阻塞投递与
/// 轻量用户事件唤醒，真正的分发在 GUI 线程完成。
fn push_host_action(
    sender: &Sender<HostAction>,
    workspace: &dyn WorkspaceDriver,
    action: HostAction,
) {
    match sender.try_send(action) {
        Ok(()) => workspace.wake_host(),
        Err(TrySendError::Full(action)) => {
            tracing::warn!("宿主动作队列已满，已丢弃本次动作: {action:?}");
        }
        Err(TrySendError::Closed(action)) => {
            tracing::debug!("贴图工作区已关闭，宿主动作已丢弃: {action:?}");
        }
    }
}

/// 构建 GUI 线程宿主唤醒回调：排空动作队列并分发，原生对象随闭包存活到退出。
pub(super) fn build_host_wake(
    tray: Option<PlatformTray>,
    hotkey_listener: Option<PlatformHotkeyListener>,
    mut receiver: Receiver<HostAction>,
    services: Arc<AppServices>,
    workspace: Arc<dyn WorkspaceDriver>,
) -> Option<Box<dyn FnMut()>> {
    if tray.is_none() && hotkey_listener.is_none() {
        return None;
    }
    Some(Box::new(move || {
        // 原生托盘与热键管理器必须活到事件循环退出；动作由构造时注册的回调写入队列。
        let _keep_alive = (&tray, &hotkey_listener);
        while let Ok(action) = receiver.try_recv() {
            let keep_running = match action {
                HostAction::Tray(action) => handle_tray_action(action, &services, &workspace),
                HostAction::Hotkey(action) => handle_hotkey_action(action, &services, &workspace),
            };
            if !keep_running {
                workspace.request_shutdown();
                break;
            }
        }
    }))
}
