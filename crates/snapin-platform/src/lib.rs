//! 平台特定系统集成层（Ubuntu 26 GNOME / Wayland）。

mod autostart;
mod capabilities;
mod capture;
mod clipboard;
mod command;
mod error;
#[cfg(target_os = "linux")]
mod extension_client;
mod hotkey_manager;
mod lifecycle;
mod notification;
#[cfg(target_os = "linux")]
mod portal;
mod tray;
mod window;

#[cfg(target_os = "linux")]
pub(crate) mod dbus_session;

mod dialog;

pub use autostart::sync_autostart;
pub use capabilities::{
    get_platform_capabilities, init_platform_capabilities, refresh_platform_capabilities,
    PlatformCapabilities,
};
pub use capture::capture_fullscreen_snapshot;
pub use clipboard::{
    read_clipboard_image, read_clipboard_text, subscribe_clipboard_changes,
    write_image_to_clipboard, write_raw_image_to_clipboard, write_text_to_clipboard,
    ClipboardListenerGuard,
};
pub use command::output_with_timeout;
pub use error::PlatformError;
pub use window::WindowRect;

#[cfg(target_os = "linux")]
pub use extension_client::{capture_fullscreen_extension, list_windows, place_paster_window};

pub use hotkey_manager::{
    hotkey_to_gnome_accelerator, sync_global_hotkeys, sync_hotkeys_to_gnome, HotkeyAction,
    PlatformHotkeyListener,
};
pub use lifecycle::{
    open_path_in_file_manager, open_url, run_bounded_system_command, spawn_independent_process,
    start_daemon_process, stop_systemd_service,
};
pub use notification::send_desktop_notification;

pub use dialog::{pick_config_export_path, pick_config_import_path, pick_save_path};
#[cfg(target_os = "linux")]
pub use portal::{capture_fullscreen_portal, ensure_portal_screenshot_permission};
pub use tray::{PlatformTray, TrayAction};
pub use window::enumerate_visible_windows;
