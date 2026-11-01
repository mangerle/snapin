//! 截图与取色触发模式领域枚举。

use serde::{Deserialize, Serialize};

/// 截图触发模式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnipMode {
    /// 常规交互截图
    #[default]
    Normal,
    /// 截屏并自动复制（选定区域后立即自动复制并退出，无需点击确认）
    AutoCopy,
    /// 框选后自动翻译（首次锁定选区时立即执行整图翻译，保留工具栏便于继续处理）
    Translate,
    /// 自定义截屏（优先复用上次历史选区）
    Custom,
    /// 全屏截图（进入会话即锁定整屏选区，工具栏可继续标注或复制/保存/贴图）
    Fullscreen,
    /// 全屏取色器（无选区，单击复制光标处颜色并退出）
    PickColor,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// IPC 协议依赖稳定的 snake_case 表示，任何变体改名都必须显式同步守护进程客户端。
    #[test]
    fn test_snip_mode_serde_names_are_stable() {
        let cases = [
            (SnipMode::Normal, "\"normal\""),
            (SnipMode::AutoCopy, "\"auto_copy\""),
            (SnipMode::Translate, "\"translate\""),
            (SnipMode::Custom, "\"custom\""),
            (SnipMode::Fullscreen, "\"fullscreen\""),
            (SnipMode::PickColor, "\"pick_color\""),
        ];
        for (mode, expected) in cases {
            let encoded = serde_json::to_string(&mode).expect("触发模式应可序列化");
            assert_eq!(encoded, expected);
            let decoded: SnipMode = serde_json::from_str(&encoded).expect("触发模式应可反序列化");
            assert_eq!(decoded, mode);
        }
    }
}
