//! 原位翻译的共享表现契约。

use serde::{Deserialize, Serialize};

/// 原位翻译背景覆盖模式。
///
/// 该枚举同时被配置持久化与标注图元绘制使用，放在基础契约层避免标注 crate
/// 为获取一个纯表现枚举而反向依赖整份配置模型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CoverMode {
    /// 智能取色实色填充
    #[default]
    SolidColor,
    /// 半透明磨砂覆盖
    FrostedGlass,
}
