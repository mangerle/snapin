//! 平台特定系统集成层（Ubuntu 26 GNOME / Wayland）。

pub mod extension_client;
pub mod hotkey_manager;
pub mod notification;
pub mod portal;

pub use extension_client::pin_window_above;
pub use hotkey_manager::sync_hotkeys_to_gnome;
pub use notification::send_desktop_notification;
pub use portal::capture_fullscreen_portal;
