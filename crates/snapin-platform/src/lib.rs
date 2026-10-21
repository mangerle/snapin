//! 平台特定系统集成层（Ubuntu 26 GNOME / Wayland）。

pub mod autostart;
pub mod capabilities;
pub mod clipboard;
pub mod error;
pub mod extension_client;
pub mod hotkey_manager;
pub mod lifecycle;
pub mod notification;
pub mod portal;
pub mod storage;

pub(crate) mod dbus_session;

pub use autostart::sync_autostart_desktop_file;
pub use capabilities::{
    get_platform_capabilities, init_platform_capabilities, refresh_platform_capabilities,
    PlatformCapabilities,
};
pub use clipboard::{
    read_clipboard_image, read_clipboard_text, write_image_to_clipboard,
    write_raw_image_to_clipboard, write_text_to_clipboard,
};
pub use error::PlatformError;
pub use extension_client::{capture_fullscreen_extension, place_paster_window};
pub use hotkey_manager::{
    hotkey_to_gnome_accelerator, suspend_hotkeys_for_settings, sync_hotkeys_to_gnome,
};
pub use lifecycle::{
    enable_gnome_extension, open_path_in_file_manager, spawn_detached_system_command,
    stop_systemd_service,
};
pub use notification::send_desktop_notification;
pub use portal::capture_fullscreen_portal;
pub use storage::{
    checked_rgba_byte_len, clear_selection_cache, load_last_selection, resolve_save_path,
    save_image_with_config, save_last_selection, MAX_RAW_PIXELS,
};
