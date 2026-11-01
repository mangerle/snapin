//! 设置中心配置与 Slint 界面之间的双向同步。
//!
//! 写界面与读界面拆分为独立模块，门面只暴露两个方向入口；
//! 所有枚举索引换算继续集中在 `mapping`，避免界面与配置模型互相泄漏细节。

mod apply;
mod extract;
mod scalar_table;

pub(crate) use apply::sync_config_to_window;
pub(crate) use extract::extract_config_from_window;

#[cfg(test)]
mod tests;
