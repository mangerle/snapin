//! 截图流水线调度、截图覆盖层准备与结果输出。
//!
//! 所有截图覆盖层由常驻守护进程的单一 GUI 事件循环托管；本模块负责捕获屏幕、
//! 枚举窗口候选、构造覆盖层启动参数，并把结果转换为通知或贴图窗口。

use crate::placement::{confirm_paster_placement, register_paster_placement};
use crate::services::AppServices;
use anyhow::{Context, Result};
use snapin_config::config::Config;
use snapin_foundation::{PostCaptureAction, SnipMode};
use snapin_platform::{
    capture_fullscreen_snapshot, enumerate_visible_windows, send_desktop_notification,
    WindowRect as PlatformWindowRect,
};
use snapin_ui::snipper::{SnipperLaunch, SnipperOutcome, SnipperRuntime, WindowRect};
use snapin_ui::workspace::PasterWorkspaceProxy;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 全屏快照捕获的端到端上限
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(20);

/// 一次截图启动请求。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SnipStartRequest {
    /// 屏幕捕获与选区模式
    pub(crate) mode: SnipMode,
    /// 截图前延时秒数
    pub(crate) delay_seconds: u32,
    /// 首次选区锁定后自动执行的动作
    pub(crate) post_action: Option<PostCaptureAction>,
}

/// 获取全屏快照（跨平台调度：Linux 优先扩展零白闪/Portal 降级，Windows/macOS 走原生捕获）。
///
/// # Errors
///
/// 平台截屏通道不可用、超时或图像解码失败时返回错误。
pub async fn acquire_screen_snapshot(include_cursor: bool) -> Result<image::RgbaImage> {
    let t_start = Instant::now();
    let rgba = tokio::time::timeout(
        SCREENSHOT_TIMEOUT,
        capture_fullscreen_snapshot(include_cursor),
    )
    .await
    .context("屏幕快照捕获超时")?
    .context("屏幕快照捕获失败")?;

    tracing::info!(
        "全屏快照捕获成功，图像尺寸: {}x{}，耗时: {:.2}ms",
        rgba.width(),
        rgba.height(),
        Instant::now()
            .saturating_duration_since(t_start)
            .as_secs_f64()
            * 1000.0
    );
    Ok(rgba)
}

/// 准备一次截图覆盖层所需的底图、窗口候选与应用端口。
///
/// `delay_seconds` 大于 0 时先发送通知并等待；等待期间监听统一取消令牌，
/// 守护进程停机时可以及时放弃本次截图。
///
/// # Errors
///
/// 延时期间被取消、屏幕捕获失败或应用服务不可用时返回错误。
pub(crate) async fn prepare_snipper_launch(
    services: &Arc<AppServices>,
    request: SnipStartRequest,
) -> Result<SnipperLaunch> {
    wait_snipper_delay(services, request.delay_seconds).await?;

    let t0 = Instant::now();
    let config = services.config();
    let bg_image = acquire_screen_snapshot(config.interface.capture_mouse_cursor).await?;
    tracing::info!(
        "快照准备就绪: {}x{}，耗时: {:.2}ms，正在呈现交互覆盖层...",
        bg_image.width(),
        bg_image.height(),
        t0.elapsed().as_secs_f64() * 1000.0
    );

    let windows = if matches!(
        request.mode,
        SnipMode::Normal | SnipMode::AutoCopy | SnipMode::Translate
    ) {
        match tokio::time::timeout(Duration::from_millis(500), enumerate_visible_windows()).await {
            Ok(Ok(windows)) => windows
                .into_iter()
                .map(to_ui_window_rect)
                .collect::<Vec<WindowRect>>(),
            Ok(Err(error)) => {
                tracing::debug!("获取窗口截图候选列表失败，继续使用普通区域截图: {error}");
                Vec::new()
            }
            Err(_) => {
                tracing::debug!("获取窗口截图候选列表超时，继续使用普通区域截图");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    Ok(SnipperLaunch {
        bg_image,
        windows,
        runtime: SnipperRuntime {
            config: (*config).clone(),
            mode: request.mode,
            post_action: request.post_action,
            desktop: services.desktop(),
            media: services.media(),
        },
    })
}

/// 延时截图前发送通知并等待，等待期间响应应用运行时取消信号。
async fn wait_snipper_delay(services: &Arc<AppServices>, delay_seconds: u32) -> Result<()> {
    if delay_seconds == 0 {
        return Ok(());
    }
    let message =
        snapin_foundation::trf!("将在 {} 秒后截图", "Capturing in {} seconds", delay_seconds);
    if let Err(error) =
        send_desktop_notification(snapin_foundation::i18n::tr("Snapin 截图"), &message).await
    {
        tracing::debug!("发送延时截图通知失败或系统通知不可用: {error}");
    }
    tracing::info!("截图延时等待中: {delay_seconds} 秒");
    let cancel = services.runtime().cancellation_token();
    tokio::select! {
        _ = cancel.cancelled() => anyhow::bail!("截图延时等待被取消"),
        _ = tokio::time::sleep(Duration::from_secs(u64::from(delay_seconds))) => Ok(()),
    }
}

/// 在常驻 GUI 工作区中异步启动截图覆盖层。
///
/// 截屏与窗口枚举在应用运行时执行，完成后通过事件循环代理投递覆盖层；
/// 任务受统一运行时追踪，守护进程停机时会等待其结束。
pub(crate) async fn start_snipper_task(
    services: Arc<AppServices>,
    workspace: PasterWorkspaceProxy,
    request: SnipStartRequest,
) {
    match prepare_snipper_launch(&services, request).await {
        Ok(launch) => {
            if let Err(error) = workspace.open_snipper(launch) {
                tracing::warn!("向常驻工作区投递截图覆盖层失败: {error}");
                let _ = send_desktop_notification(
                    snapin_foundation::i18n::tr("Snapin 截图失败"),
                    snapin_foundation::i18n::tr(
                        "无法在常驻界面中打开截图覆盖层，详情请查看运行日志",
                    ),
                )
                .await;
            }
        }
        Err(error) => {
            tracing::warn!("准备截图覆盖层失败: {error:#}");
            let _ = send_desktop_notification(
                snapin_foundation::i18n::tr("Snapin 截图失败"),
                snapin_foundation::i18n::tr("屏幕捕获或截图初始化失败，详情请查看运行日志"),
            )
            .await;
        }
    }
}

/// 处理截图结果：通知、贴图交付与错误提示。
///
/// 截图覆盖层运行在常驻守护进程的 GUI 事件循环中，结果可直接投递到同一工作区
/// 创建贴图窗口，不再需要临时像素文件或独立贴图子进程。
///
/// # Errors
///
/// 结果是贴图且向常驻工作区投递失败时返回错误。
pub(crate) async fn handle_snipper_outcome(
    config: &Config,
    outcome: SnipperOutcome,
    workspace: &PasterWorkspaceProxy,
) -> Result<()> {
    match outcome {
        SnipperOutcome::CopiedImage => {
            if config.output.notify_on_copy {
                notify_or_debug("Snapin 截图", "选区图像已成功复制到剪贴板").await;
            }
        }
        SnipperOutcome::CopiedText(text) => {
            if config.output.notify_on_copy {
                let summary = format!("识别文字已复制到剪贴板（{} 字符）", text.chars().count());
                notify_or_debug("Snapin 文字识别", &summary).await;
            }
        }
        SnipperOutcome::CopiedColor(color) => {
            if config.output.notify_on_copy {
                let summary = format!("颜色值已复制到剪贴板: {color}");
                notify_or_debug("Snapin 取色器", &summary).await;
            }
        }
        SnipperOutcome::Saved(path) => {
            if config.output.notify_on_save {
                let message = format!("选区图像已保存至: {}", path.display());
                notify_or_debug("Snapin 截图已保存", &message).await;
            }
        }
        SnipperOutcome::Pinned(item) => {
            let title = item.title.clone();
            let (x, y) = item.position;
            // Wayland 下客户端无法自定位：必须先把选区物理坐标登记给扩展，
            // 再投递窗口，最后轮询确认，否则贴图会落到合成器默认位置。
            register_paster_placement(&title, x, y).await;
            workspace
                .open_paster(item)
                .context("向常驻集中工作区投递贴图失败")?;
            confirm_paster_placement(&title, x, y).await;
        }
        SnipperOutcome::Cancelled => {
            tracing::info!("用户已取消本次截图");
        }
    }
    Ok(())
}

/// 发送非关键桌面通知；失败只记录调试日志，不影响已经完成的业务结果。
async fn notify_or_debug(summary: &str, body: &str) {
    if let Err(error) = send_desktop_notification(summary, body).await {
        tracing::debug!("发送桌面通知失败或系统通知不可用: {error}");
    }
}

/// 将平台窗口矩形转换为 UI 几何契约，避免应用层依赖 UI 内部类型。
fn to_ui_window_rect(rect: PlatformWindowRect) -> WindowRect {
    WindowRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}
