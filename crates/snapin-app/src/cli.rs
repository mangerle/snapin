//! 命令行运维与应用命令编排。
//!
//! 二进制入口只负责解析参数、配置日志与进程级路由；截图模式选择、守护进程
//! 自动拉起、贴图分组校验和运维命令执行统一由本模块完成，避免入口层重复承担
//! 应用业务决策。

use anyhow::{bail, Context, Result};
use snapin_config::config::Config;
use snapin_daemon::{send_command_to_daemon, DaemonRequest};
use snapin_foundation::{PostCaptureAction, SnipMode};
use std::path::PathBuf;

/// 一次截图命令的完整选项。
#[derive(Debug, Clone, Copy)]
pub struct SnipOptions {
    /// 截图完成后自动复制。
    pub auto_copy: bool,
    /// 恢复并编辑自定义选区。
    pub custom: bool,
    /// 截图后进入翻译流程。
    pub translate: bool,
    /// 直接截取全屏。
    pub fullscreen: bool,
    /// 截图后进入智能打码。
    pub redact: bool,
    /// 截图后识别二维码。
    pub qr: bool,
    /// 截图后进行美化导出。
    pub beautify: bool,
    /// 截图后打开 OCR 结果面板。
    pub ocr_panel: bool,
    /// 截图前延时秒数。
    pub delay_seconds: u32,
}

impl SnipOptions {
    /// 将互斥选项收敛为截图模式。
    fn mode(self) -> SnipMode {
        if self.fullscreen {
            SnipMode::Fullscreen
        } else if self.translate {
            SnipMode::Translate
        } else if self.auto_copy {
            SnipMode::AutoCopy
        } else if self.custom {
            SnipMode::Custom
        } else {
            SnipMode::Normal
        }
    }

    /// 将互斥后处理选项收敛为单一动作。
    fn post_action(self) -> Option<PostCaptureAction> {
        if self.redact {
            Some(PostCaptureAction::SmartRedact)
        } else if self.qr {
            Some(PostCaptureAction::Qr)
        } else if self.beautify {
            Some(PostCaptureAction::Beautify)
        } else if self.ocr_panel {
            Some(PostCaptureAction::OcrPanel)
        } else {
            None
        }
    }
}

/// 不依赖 GUI 主线程的命令行应用操作。
#[derive(Debug)]
pub enum ApplicationCommand {
    /// 启动截图覆盖层。
    Snip(SnipOptions),
    /// 启动取色覆盖层。
    PickColor,
    /// 从剪贴板生成贴图。
    Paste,
    /// 切换当前分组贴图显隐。
    TogglePasters,
    /// 切换贴图分组。
    SwitchGroup(Option<usize>),
    /// 请求打开贴图工作区。
    Workspace,
    /// 清理截图历史与选区缓存。
    ClearHistory,
    /// 请求守护进程平滑退出。
    Exit,
    /// 检查或安装更新。
    Update {
        /// 是否只检查而不下载安装。
        check_only: bool,
    },
    /// 从外部图像文件创建贴图。
    PinFile {
        /// 图像文件路径。
        path: PathBuf,
        /// 初始横坐标。
        x: i32,
        /// 初始纵坐标。
        y: i32,
    },
}

/// 执行一个应用命令行操作。
///
/// # Errors
///
/// 守护进程启动或通信、输入参数校验、历史清理、更新、退出或贴图创建失败时返回错误。
pub async fn run_application_command(
    command: ApplicationCommand,
    config: Option<Config>,
) -> Result<()> {
    match command {
        ApplicationCommand::Snip(options) => send_snip(options).await,
        ApplicationCommand::PickColor => {
            tracing::info!("正在请求常驻守护服务启动取色覆盖层...");
            ensure_daemon_running().await?;
            send_or_fail(DaemonRequest::StartSnip {
                mode: SnipMode::PickColor,
                delay_seconds: 0,
                post_action: None,
            })
            .await
        }
        ApplicationCommand::Paste => {
            tracing::info!("正在请求常驻守护服务检查剪贴板生成贴图...");
            ensure_daemon_running().await?;
            send_or_fail(DaemonRequest::Paste).await
        }
        ApplicationCommand::TogglePasters => {
            if !send_or_absent(DaemonRequest::TogglePasters).await? {
                tracing::info!("未检测到常驻守护服务，无活动贴图需要切换");
            }
            Ok(())
        }
        ApplicationCommand::SwitchGroup(id) => switch_group(id).await,
        ApplicationCommand::Workspace => {
            ensure_daemon_running().await?;
            send_or_fail(DaemonRequest::OpenWorkspace).await
        }
        ApplicationCommand::ClearHistory => {
            if !send_or_absent(DaemonRequest::ClearHistory).await? {
                crate::clear_snip_history().await?;
            }
            Ok(())
        }
        ApplicationCommand::Exit => crate::exit_snapin_application().await,
        ApplicationCommand::Update { check_only } => {
            crate::run_cli_update(config, check_only).await
        }
        ApplicationCommand::PinFile { path, x, y } => crate::run_pin_file(path, x, y).await,
    }
}

/// 探测常驻守护进程是否在线。
pub async fn is_daemon_online() -> bool {
    matches!(send_command_to_daemon(&DaemonRequest::Ping).await, Ok(true))
}

/// 启动常驻守护进程中的截图覆盖层。
async fn send_snip(options: SnipOptions) -> Result<()> {
    tracing::info!("正在请求常驻守护服务启动截图覆盖层...");
    ensure_daemon_running().await?;
    send_or_fail(DaemonRequest::StartSnip {
        mode: options.mode(),
        delay_seconds: options.delay_seconds,
        post_action: options.post_action(),
    })
    .await
}

/// 校验并切换贴图分组。
async fn switch_group(id: Option<usize>) -> Result<()> {
    if let Some(group) = id {
        if !snapin_ui::workspace::is_valid_group_id(snapin_ui::paster::GroupId::new(group)) {
            bail!(
                "贴图分组 ID 越界: {group}，有效范围: 0..={}",
                snapin_ui::workspace::TOTAL_PIN_GROUPS.saturating_sub(1)
            );
        }
    }
    if !send_or_absent(DaemonRequest::SwitchGroup(id)).await? {
        tracing::info!("未检测到常驻守护服务，无活动贴图分组需要切换");
    }
    Ok(())
}

/// 确保常驻守护进程在线。
async fn ensure_daemon_running() -> Result<()> {
    crate::ensure_daemon_running().await
}

/// 发送请求并要求守护进程明确成功。
async fn send_or_fail(request: DaemonRequest) -> Result<()> {
    match send_command_to_daemon(&request).await {
        Ok(true) => {
            tracing::info!("已通过 IPC 委托常驻守护服务处理请求");
            Ok(())
        }
        Ok(false) => bail!("常驻守护服务未响应请求"),
        Err(error) => Err(error).context("常驻守护服务处理请求失败"),
    }
}

/// 发送请求；守护进程缺席时返回 `false` 而不是错误。
async fn send_or_absent(request: DaemonRequest) -> Result<bool> {
    match send_command_to_daemon(&request).await {
        Ok(handled) => Ok(handled),
        Err(error) => Err(error).context("常驻守护服务处理请求失败"),
    }
}

/// 初始化独立设置中心所需平台能力快照。
pub async fn initialize_platform_capabilities() {
    let _ = snapin_platform::init_platform_capabilities().await;
}

/// 将当前快捷键配置同步到系统。
///
/// # Errors
///
/// 平台快捷键后端不可用或写入配置失败时返回错误。
pub fn sync_system_hotkeys(config: &Config) -> Result<()> {
    snapin_platform::sync_hotkeys_to_gnome(&config.hotkeys).context("同步快捷键至 GNOME 失败")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 截图模式与后处理动作必须保持互斥选项的确定优先级。
    #[test]
    fn test_snip_options_map_to_single_mode_and_action() {
        let options = SnipOptions {
            auto_copy: true,
            custom: false,
            translate: true,
            fullscreen: false,
            redact: false,
            qr: true,
            beautify: false,
            ocr_panel: false,
            delay_seconds: 3,
        };
        assert_eq!(options.mode(), SnipMode::Translate);
        assert_eq!(options.post_action(), Some(PostCaptureAction::Qr));
    }
}
