//! Snapin 命令行参数与子命令模型定义。

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// 命令行参数结构体
#[derive(Parser, Debug)]
#[command(
    name = "snapin",
    author = "wangle",
    version = env!("CARGO_PKG_VERSION"),
    about = "专为 Linux Wayland (Ubuntu 26 GNOME) 打造的纯 Rust 截图与贴图工具",
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
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
    },
    /// 触发贴图（由剪贴板或选区生成，默认绑定 F1）
    Paste,
    /// 批量显示/隐藏当前活动组所有贴图 (默认绑定 Ctrl+F3)
    Toggle,
    /// 切换贴图活动分组（未指定 ID 时循环轮转至下一个分组）
    SwitchGroup {
        /// 目标分组 ID（可选）
        id: Option<usize>,
    },
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
}

impl Commands {
    /// 该子命令是否强依赖用户配置文件。
    ///
    /// 只有真正读取偏好字段的命令才需要读盘结果：截图依赖热键之外的全部设置，
    /// 快捷键同步依赖热键表，守护进程依赖贴图与输出设置。
    pub fn needs_config(&self) -> bool {
        matches!(self, Self::Snip { .. } | Self::SyncHotkeys | Self::Daemon)
    }

    /// 该子命令是否需要预先探测桌面平台能力（GNOME 特权扩展在线状态）。
    ///
    /// 探测会发起一次 D-Bus 往返，两条路径消费其结果：截图据此决定是否走扩展零白闪原生通道
    /// （扩展未在线时直接走 portal，避免一次注定失败的调用），设置中心据此展示扩展部署状态。
    pub fn needs_capabilities(&self) -> bool {
        matches!(self, Self::Snip { .. } | Self::Prefs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// 断言启动副作用裁剪策略：仅强依赖偏好与桌面探测的命令返回 true
    #[test]
    fn test_startup_dependency_predicates() {
        Cli::command().debug_assert();

        let config_heavy = [
            Commands::Snip {
                auto_copy: false,
                custom: false,
                translate: false,
            },
            Commands::SyncHotkeys,
            Commands::Daemon,
        ];
        for command in config_heavy {
            assert!(command.needs_config(), "{command:?} 应强依赖配置");
        }

        let config_free = [
            Commands::Paste,
            Commands::Toggle,
            Commands::SwitchGroup { id: None },
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

        assert!(Commands::Prefs.needs_capabilities());
        assert!(Commands::Snip {
            auto_copy: true,
            custom: false,
            translate: false,
        }
        .needs_capabilities());
        assert!(!Commands::Exit.needs_capabilities());
        assert!(!Commands::Daemon.needs_capabilities());
    }

    /// 裸命令必须给出用法提示而非静默拉起守护进程
    #[test]
    fn test_bare_invocation_requires_subcommand() {
        let error = Cli::try_parse_from(["snapin"]).expect_err("无子命令时不应解析成功");
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
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
        let cli = Cli::try_parse_from(["snapin", "snip", "--custom"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Snip {
                auto_copy: false,
                custom: true,
                translate: false,
            }
        ));

        let cli = Cli::try_parse_from(["snapin", "snip", "--translate"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Snip {
                translate: true,
                ..
            }
        ));

        assert!(
            Cli::try_parse_from(["snapin", "snip", "--translate", "--custom"]).is_err(),
            "自动翻译与自定义选区模式互斥，不能同时指定"
        );

        let cli = Cli::try_parse_from(["snapin", "pin-file", "/tmp/a.raw", "10", "20"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::PinFile { x: 10, y: 20, .. }
        ));
    }
}
