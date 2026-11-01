//! 平台特定系统集成层（Ubuntu 26 GNOME / Wayland）。

pub mod autostart;
pub mod capabilities;
pub mod capture;
pub mod clipboard;
pub mod error;
#[cfg(target_os = "linux")]
pub mod extension_client;
pub mod hotkey_manager;
pub mod lifecycle;
pub mod notification;
#[cfg(target_os = "linux")]
pub mod portal;
pub mod storage;
pub mod tray;
pub mod window;

#[cfg(target_os = "linux")]
pub(crate) mod dbus_session;

#[cfg(not(target_os = "linux"))]
pub mod dialog;

pub use autostart::sync_autostart;
pub use capabilities::{
    get_platform_capabilities, init_platform_capabilities, refresh_platform_capabilities,
    PlatformCapabilities,
};
pub use capture::capture_fullscreen_snapshot;
pub use clipboard::{
    read_clipboard_image, read_clipboard_text, write_image_to_clipboard,
    write_raw_image_to_clipboard, write_text_to_clipboard,
};
pub use error::PlatformError;
pub use window::WindowRect;

#[cfg(target_os = "linux")]
pub use extension_client::{capture_fullscreen_extension, list_windows, place_paster_window};

#[cfg(not(target_os = "linux"))]
pub async fn place_paster_window(
    _title: &str,
    _x: i32,
    _y: i32,
) -> Result<bool, error::PlatformError> {
    Ok(false)
}

#[cfg(not(target_os = "linux"))]
pub async fn capture_fullscreen_extension(
    _target_path: &str,
) -> Result<bool, error::PlatformError> {
    Ok(false)
}

#[cfg(not(target_os = "linux"))]
pub async fn list_windows() -> Result<Vec<window::WindowRect>, error::PlatformError> {
    Ok(vec![])
}

pub use hotkey_manager::{
    hotkey_to_gnome_accelerator, suspend_hotkeys_for_settings, sync_global_hotkeys,
    sync_hotkeys_to_gnome, HotkeyAction, PlatformHotkeyListener,
};
pub use lifecycle::{
    enable_gnome_extension, open_path_in_file_manager, spawn_detached_system_command,
    stop_systemd_service,
};
pub use notification::send_desktop_notification;

#[cfg(not(target_os = "linux"))]
pub use dialog::pick_save_path;
#[cfg(target_os = "linux")]
pub use portal::{capture_fullscreen_portal, ensure_portal_screenshot_permission, pick_save_path};
pub use storage::{
    checked_rgba_byte_len, clear_selection_cache, load_last_selection, resolve_save_path,
    save_image_to_path, save_image_with_config, save_last_selection, MAX_RAW_PIXELS,
};
pub use tray::{PlatformTray, TrayAction};
pub use window::enumerate_visible_windows;
