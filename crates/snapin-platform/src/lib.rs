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
pub mod tray;
pub mod window;

#[cfg(target_os = "linux")]
pub(crate) mod dbus_session;

#[cfg(any(target_os = "windows", target_os = "macos"))]
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

pub use hotkey_manager::{
    hotkey_to_gnome_accelerator, sync_global_hotkeys, sync_hotkeys_to_gnome, HotkeyAction,
    PlatformHotkeyListener,
};
pub use lifecycle::{
    enable_gnome_extension, open_path_in_file_manager, spawn_detached_system_command,
    stop_systemd_service,
};
pub use notification::send_desktop_notification;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use dialog::pick_save_path;
#[cfg(target_os = "linux")]
pub use portal::{capture_fullscreen_portal, ensure_portal_screenshot_permission, pick_save_path};

/// 未接入原生保存对话框的平台显式返回不支持，而不是伪造成功空值。
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub async fn pick_save_path(
    _suggested_name: &str,
    _current_folder: Option<&std::path::Path>,
) -> Result<Option<std::path::PathBuf>, error::PlatformError> {
    Err(error::PlatformError::Unsupported {
        capability: "系统保存对话框".to_string(),
    })
}
pub use tray::{PlatformTray, TrayAction};
pub use window::enumerate_visible_windows;
