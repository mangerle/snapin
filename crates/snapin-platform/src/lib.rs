//! 平台特定系统集成层（Ubuntu 26 GNOME / Wayland）。

pub mod autostart;
pub mod clipboard;
pub mod extension_client;
pub mod hotkey_manager;
pub mod lifecycle;
pub mod notification;
pub mod portal;
pub mod save_helper;

pub use autostart::sync_autostart_desktop_file;
pub use clipboard::{
    read_clipboard_image, read_clipboard_text, write_image_to_clipboard, write_text_to_clipboard,
};
pub use extension_client::{capture_fullscreen_extension, pin_window_above};
pub use hotkey_manager::{
    convert_to_gnome_accelerator, suspend_hotkeys_for_settings, sync_hotkeys_to_gnome,
};
pub use lifecycle::{disable_gnome_extension, stop_systemd_service};
pub use notification::send_desktop_notification;
pub use portal::capture_fullscreen_portal;
pub use save_helper::{
    clear_selection_cache, generate_secure_temp_path, generate_unique_id, get_secure_runtime_dir,
    load_last_selection, resolve_save_path, save_image_with_config, save_last_selection,
};
