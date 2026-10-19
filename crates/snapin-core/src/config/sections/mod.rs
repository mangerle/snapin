//! 各配置板块子模块聚合门面。

pub mod general;
pub mod hotkeys;
pub mod interface;
pub mod output;
pub mod paster;
pub mod snip;

pub use general::GeneralConfig;
pub use hotkeys::HotkeyConfig;
pub use interface::InterfaceConfig;
pub use output::OutputConfig;
pub use paster::PasterConfig;
pub use snip::SnipConfig;
