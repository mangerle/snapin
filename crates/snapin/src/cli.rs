//! Snapin 命令行参数与子命令模型定义。

use clap::{Parser, Subcommand};
use snapin_app::{ApplicationCommand, SnipOptions};
use std::path::PathBuf;

/// 命令行参数结构体
#[derive(Parser, Debug)]
#[command(
    name = "snapin",
    author = "wangle",
    version = env!("CARGO_PKG_VERSION"),
    about = "专为 Linux Wayland (Ubuntu 26 GNOME) 打造的纯 Rust 截图与贴图工具",
    arg_required_else_help = false
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// 支持的子命令定义
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 触发区域截图流水线 (默认绑定 Ctrl+Alt+A)
    Snip {
        /// 截图并自动复制到剪贴板
        #[arg(long)]
        auto_copy: bool,
        /// 自定义截图（加载历史选区）
        #[arg(long)]
        custom: bool,
        /// 框选完成后自动对选区执行整图翻译（由截图翻译热键触发）
        #[arg(long, conflicts_with_all = ["auto_copy", "custom"])]
        translate: bool,
        /// 进入会话即锁定整屏选区，可直接标注或复制/保存/贴图
        #[arg(long, conflicts_with_all = ["auto_copy", "custom", "translate"])]
        fullscreen: bool,
        /// 框选后识别全部文字并进入智能打码预览
        #[arg(long, conflicts_with_all = ["auto_copy", "custom", "translate", "fullscreen", "qr", "beautify", "ocr_panel"])]
        redact: bool,
        /// 框选后自动识别二维码
        #[arg(long, conflicts_with_all = ["auto_copy", "custom", "translate", "fullscreen", "redact", "beautify", "ocr_panel"])]
        qr: bool,
        /// 框选后进入美化导出
        #[arg(long, conflicts_with_all = ["auto_copy", "custom", "translate", "fullscreen", "redact", "qr", "ocr_panel"])]
        beautify: bool,
        /// 框选后自动 OCR 并打开全文编辑框
        #[arg(long, conflicts_with_all = ["auto_copy", "custom", "translate", "fullscreen", "redact", "qr", "beautify"])]
        ocr_panel: bool,
        /// 截图前等待的秒数（0 表示立即截图，最大 60 秒）
        #[arg(long, value_name = "SECONDS", default_value_t = 0, value_parser = parse_delay_seconds)]
        delay: u32,
    },
    /// 触发贴图（由剪贴板或选区生成，默认绑定 F1）
    Paste,
    /// 打开全屏取色器，单击复制当前颜色并退出
    PickColor,
    /// 批量显示/隐藏当前活动组所有贴图 (默认绑定 Ctrl+F3)
    Toggle,
    /// 切换贴图活动分组（未指定 ID 时循环轮转至下一个分组）
    SwitchGroup {
        /// 目标分组 ID（省略时循环切换；越界 ID 会被拒绝）
        id: Option<usize>,
    },
    /// 显示当前活动分组的全部贴图
    Workspace,
    /// 打开或关闭剪贴板历史面板
    ClipboardHistory,
    /// 清空截屏与临时缓存历史
    ClearHistory,
    /// 完全退出 Snapin（停止守护服务、关闭贴图并收起托盘图标）
    Exit,
    /// 在前台启动常驻守护进程
    Daemon,
    /// 独立启动一个置顶贴图窗口展示指定图像文件
    PinFile {
        /// 贴图图像路径
        path: PathBuf,
        /// 初始 X 坐标
        #[arg(default_value_t = 200)]
        x: i32,
        /// 初始 Y 坐标
        #[arg(default_value_t = 200)]
        y: i32,
    },
    /// 打开 Snapin 首选项 / 设置页面
    Prefs,
    /// 部署配套 GNOME Shell 扩展以获得置顶特权
    InstallExtension,
    /// 向系统 GNOME 快捷键中心同步绑定默认热键
    SyncHotkeys,
    /// 检查或执行 Snapin 在线自更新
    Update {
        /// 仅检查最新版本信息，不下载也不安装
        #[arg(long)]
        check: bool,
    },
}

/// 解析截图延时秒数，限制在 0..=60，避免误输入导致长时间无响应。
fn parse_delay_seconds(raw: &str) -> Result<u32, String> {
    let seconds = raw
        .parse::<u32>()
        .map_err(|error| format!("截图延时必须是 0 到 60 的整数，原因: {error}"))?;
    if seconds > 60 {
        return Err("截图延时不能超过 60 秒".to_string());
    }
    Ok(seconds)
}

/// 子命令的调度路径。
///
/// 把“是否需要配置”和“由主线程同步流程还是异步 IPC 流程执行”收敛到同一张分派表，
/// 避免 `needs_config` 与 `main` 的 match 各自维护、新增命令时漏改其中一侧。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRoute {
    /// 常驻守护进程主线程
    Daemon,
    /// 独立设置中心进程
    Preferences,
    /// 同步安装 GNOME 扩展
    InstallExtension,
    /// 同步注册系统快捷键
    SyncHotkeys,
    /// 独立剪贴板历史面板
    ClipboardPanel,
    /// 短生命周期 IPC 或运维命令
    Async,
}

impl Commands {
    /// 返回该子命令唯一的调度路径。
    pub fn route(&self) -> CommandRoute {
        match self {
            Self::Daemon => CommandRoute::Daemon,
            Self::Prefs => CommandRoute::Preferences,
            Self::InstallExtension => CommandRoute::InstallExtension,
            Self::SyncHotkeys => CommandRoute::SyncHotkeys,
            Self::ClipboardHistory => CommandRoute::ClipboardPanel,
            Self::Snip { .. }
            | Self::PickColor
            | Self::Paste
            | Self::Toggle
            | Self::SwitchGroup { .. }
            | Self::Workspace
            | Self::ClearHistory
            | Self::Exit
            | Self::PinFile { .. }
            | Self::Update { .. } => CommandRoute::Async,
        }
    }

    /// 将异步子命令转换为应用层操作。
    ///
    /// 返回 `None` 表示该命令必须由守护进程主线程、设置中心、剪贴板面板或同步
    /// 平台流程处理，调用方不得把它误送入异步命令通道。
    pub fn into_application_command(self) -> Option<ApplicationCommand> {
        match self {
            Self::Snip {
                auto_copy,
                custom,
                translate,
                fullscreen,
                redact,
                qr,
                beautify,
                ocr_panel,
                delay,
            } => Some(ApplicationCommand::Snip(SnipOptions {
                auto_copy,
                custom,
                translate,
                fullscreen,
                redact,
                qr,
                beautify,
                ocr_panel,
                delay_seconds: delay,
            })),
            Self::PickColor => Some(ApplicationCommand::PickColor),
            Self::Paste => Some(ApplicationCommand::Paste),
            Self::Toggle => Some(ApplicationCommand::TogglePasters),
            Self::SwitchGroup { id } => Some(ApplicationCommand::SwitchGroup(id)),
            Self::Workspace => Some(ApplicationCommand::Workspace),
            Self::ClearHistory => Some(ApplicationCommand::ClearHistory),
            Self::Exit => Some(ApplicationCommand::Exit),
            Self::Update { check } => Some(ApplicationCommand::Update { check_only: check }),
            Self::PinFile { path, x, y } => Some(ApplicationCommand::PinFile { path, x, y }),
            Self::ClipboardHistory
            | Self::Daemon
            | Self::Prefs
            | Self::InstallExtension
            | Self::SyncHotkeys => None,
        }
    }

    /// 该子命令是否强依赖用户配置文件。
    ///
    /// 只有真正读取偏好字段的命令才需要读盘结果：快捷键同步依赖热键表，
    /// 守护进程依赖贴图、输出与更新设置，CLI 更新需要读取自定义更新源；
    /// 截图与取色已委托守护进程，无需在 CLI 进程读配置。
    pub fn needs_config(&self) -> bool {
        matches!(self, Self::Daemon | Self::SyncHotkeys | Self::Update { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// 断言启动副作用裁剪策略：仅真正消费用户配置的命令返回 true
    #[test]
    fn test_startup_dependency_predicates() {
        Cli::command().debug_assert();

        let config_heavy = [
            Commands::SyncHotkeys,
            Commands::Daemon,
            Commands::Update { check: true },
        ];
        for command in config_heavy {
            assert!(command.needs_config(), "{command:?} 应强依赖配置");
        }

        let config_free = [
            Commands::Snip {
                auto_copy: false,
                custom: false,
                translate: false,
                fullscreen: false,
                redact: false,
                qr: false,
                beautify: false,
                ocr_panel: false,
                delay: 0,
            },
            Commands::PickColor,
            Commands::Paste,
            Commands::Toggle,
            Commands::SwitchGroup { id: None },
            Commands::Workspace,
            Commands::ClipboardHistory,
            Commands::ClearHistory,
            Commands::Exit,
            Commands::Prefs,
            Commands::InstallExtension,
            Commands::PinFile {
                path: PathBuf::from("/tmp/a.png"),
                x: 0,
                y: 0,
            },
        ];
        for command in config_free {
            assert!(!command.needs_config(), "{command:?} 不应依赖配置");
        }
    }

    /// 裸命令启动应被允许（用于桌面图标双击拉起常驻守护服务）
    #[test]
    fn test_bare_invocation_allowed_for_desktop_launch() {
        let cli = Cli::try_parse_from(["snapin"]).expect("无子命令时应允许解析并调度为默认行为");
        assert!(cli.command.is_none());
    }

    /// 纯 CLI 路径（--version）应能独立解析成功
    #[test]
    fn test_version_flag_parses() {
        let error = Cli::try_parse_from(["snapin", "--version"])
            .expect_err("--version 由 clap 直接处理并退出");
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayVersion);
    }

    /// 子命令参数应正确落到强类型命令模型
    #[test]
    fn test_subcommand_parsing() {
        let cli = Cli::try_parse_from(["snapin"]).unwrap();
        assert!(cli.command.is_none(), "无参数启动应被允许并解析为 None");

        let cli = Cli::try_parse_from(["snapin", "snip", "--custom"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Snip {
                auto_copy: false,
                custom: true,
                translate: false,
                fullscreen: false,
                redact: false,
                qr: false,
                beautify: false,
                ocr_panel: false,
                delay: 0,
            })
        ));

        for flag in ["--redact", "--qr", "--beautify", "--ocr-panel"] {
            let cli = Cli::try_parse_from(["snapin", "snip", flag]).expect("后处理参数应可解析");
            assert!(matches!(cli.command, Some(Commands::Snip { .. })));
        }

        let cli = Cli::try_parse_from(["snapin", "snip", "--translate"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Snip {
                translate: true,
                ..
            })
        ));

        let cli = Cli::try_parse_from(["snapin", "snip", "--fullscreen"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Snip {
                fullscreen: true,
                ..
            })
        ));

        let cli = Cli::try_parse_from(["snapin", "snip", "--delay", "3"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Snip { delay: 3, .. })));
        assert!(
            Cli::try_parse_from(["snapin", "snip", "--delay", "61"]).is_err(),
            "截图延时超过 60 秒必须被拒绝"
        );
        assert!(
            Cli::try_parse_from(["snapin", "snip", "--delay", "abc"]).is_err(),
            "非整数延时必须被拒绝"
        );

        assert!(
            Cli::try_parse_from(["snapin", "snip", "--translate", "--custom"]).is_err(),
            "自动翻译与自定义选区模式互斥，不能同时指定"
        );
        assert!(
            Cli::try_parse_from(["snapin", "snip", "--fullscreen", "--auto-copy"]).is_err(),
            "全屏模式与自动复制模式互斥，避免自动复制标志被静默忽略"
        );
        assert!(
            Cli::try_parse_from(["snapin", "snip", "--fullscreen", "--translate"]).is_err(),
            "全屏模式与自动翻译模式互斥"
        );

        let cli = Cli::try_parse_from(["snapin", "pin-file", "/tmp/a.raw", "10", "20"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::PinFile { x: 10, y: 20, .. })
        ));

        let cli = Cli::try_parse_from(["snapin", "pick-color"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::PickColor)));
    }

    /// 异步调度路径必须都能转换为应用命令，受保护路径必须拒绝误转换。
    #[test]
    fn test_application_command_mapping_matches_async_route() {
        let async_commands = [
            Commands::Snip {
                auto_copy: false,
                custom: false,
                translate: false,
                fullscreen: false,
                redact: false,
                qr: false,
                beautify: false,
                ocr_panel: false,
                delay: 0,
            },
            Commands::PickColor,
            Commands::Paste,
            Commands::Toggle,
            Commands::SwitchGroup { id: None },
            Commands::Workspace,
            Commands::ClearHistory,
            Commands::Exit,
            Commands::PinFile {
                path: PathBuf::from("/tmp/a.png"),
                x: 0,
                y: 0,
            },
            Commands::Update { check: true },
        ];
        for command in async_commands {
            assert_eq!(command.route(), CommandRoute::Async);
            assert!(
                command.into_application_command().is_some(),
                "异步命令必须可转换为应用操作"
            );
        }

        for command in [
            Commands::Daemon,
            Commands::Prefs,
            Commands::InstallExtension,
            Commands::SyncHotkeys,
            Commands::ClipboardHistory,
        ] {
            assert!(command.into_application_command().is_none());
        }
    }

    /// 调度路径必须覆盖全部子命令，且配置依赖与路径表保持一致。
    #[test]
    fn test_command_routes_are_exhaustive() {
        assert_eq!(Commands::Daemon.route(), CommandRoute::Daemon);
        assert_eq!(Commands::Prefs.route(), CommandRoute::Preferences);
        assert_eq!(
            Commands::InstallExtension.route(),
            CommandRoute::InstallExtension
        );
        assert_eq!(Commands::SyncHotkeys.route(), CommandRoute::SyncHotkeys);
        assert_eq!(
            Commands::Update { check: false }.route(),
            CommandRoute::Async
        );
        assert_eq!(Commands::Paste.route(), CommandRoute::Async);
        assert!(Commands::Update { check: false }.needs_config());
    }
}
