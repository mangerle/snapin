//! Snapin 命令行参数与子命令模型定义。

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// 命令行参数结构体
#[derive(Parser, Debug)]
#[command(
    name = "snapin",
    author = "wangle",
    version = env!("CARGO_PKG_VERSION"),
    about = "专为 Linux Wayland (Ubuntu 26 GNOME) 打造的纯 Rust 截图与贴图工具"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// 支持的子命令定义
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 触发区域截图流水线 (默认绑定 F1)
    Snip {
        /// 截图并自动复制到剪贴板
        #[arg(long)]
        auto_copy: bool,
        /// 自定义截图（加载历史选区）
        #[arg(long)]
        custom: bool,
    },
    /// 触发贴图（由剪贴板或选区生成，默认绑定 F3）
    Paste,
    /// 批量显示/隐藏当前活动组所有贴图 (默认绑定 Ctrl+F3)
    Toggle,
    /// 切换贴图活动分组
    SwitchGroup {
        /// 目标分组 ID
        #[arg(default_value_t = 0)]
        id: usize,
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
