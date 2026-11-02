//! 贴图窗口绝对定位：向配套 GNOME 扩展登记物理像素坐标并在窗口映射后确认落点。
//!
//! Wayland 的 xdg-shell 协议禁止客户端自行决定窗口位置，`WindowBuilder::with_position`
//! 在原生会话下会被合成器直接忽略，绝对定位只能交由持有 Mutter 特权的扩展执行。
//! 扩展在 `window-created` 阶段拿到窗口时它尚未映射，此时的 `move_frame` 会被合成器
//! 静默丢弃，因此这里采用「先登记坐标、再创建窗口、最后轮询确认」三段式：
//! 登记请求由扩展挂起，窗口映射后由确认循环重复调用同一定位接口，保证落点最终生效。
//!
//! 所有失败都只影响窗口落点、不影响贴图内容，属于可降级路径，故仅记录日志而不向上抛错。

use std::time::Duration;

#[cfg(target_os = "linux")]
use tokio::time::timeout;

/// 单次扩展 D-Bus 调用的超时上限
#[cfg(target_os = "linux")]
const PLACEMENT_TIMEOUT: Duration = Duration::from_millis(400);

/// 窗口投递后的放置确认总时长上限
#[cfg(target_os = "linux")]
const PLACEMENT_CONFIRM_WINDOW: Duration = Duration::from_millis(800);

/// 放置确认的轮询间隔
#[cfg(target_os = "linux")]
const PLACEMENT_CONFIRM_INTERVAL: Duration = Duration::from_millis(40);

/// 在贴图窗口创建前向扩展登记精确物理像素坐标。
///
/// 先登记后创建可让扩展把尚未映射的请求挂起，窗口一诞生即命中，
/// 避免窗口映射与定位调用之间的竞态。
#[cfg(target_os = "linux")]
pub(crate) async fn register_paster_placement(title: &str, x: i32, y: i32) {
    match timeout(
        PLACEMENT_TIMEOUT,
        snapin_platform::place_paster_window(title, x, y),
    )
    .await
    {
        Ok(Ok(_)) => tracing::debug!("已向扩展登记贴图放置请求: {title}，坐标: ({x}, {y})"),
        Ok(Err(error)) => tracing::warn!("请求 GNOME 扩展登记贴图放置失败: {error}"),
        Err(_) => tracing::warn!("请求 GNOME 扩展登记贴图放置超时，窗口将由扩展回退定位"),
    }
}

/// 非 Linux 平台无需 GNOME 扩展介入绝对定位，客户端已自控窗口位置
#[cfg(not(target_os = "linux"))]
pub(crate) async fn register_paster_placement(_title: &str, _x: i32, _y: i32) {}

/// 在贴图窗口投递到工作区之后确认放置已真正生效。
///
/// 窗口刚创建时尚未映射，扩展此时的定位会被合成器丢弃；这里按固定间隔重试，
/// 直到扩展回报已命中窗口（`true`）或超出确认窗口。
#[cfg(target_os = "linux")]
pub(crate) async fn confirm_paster_placement(title: &str, x: i32, y: i32) {
    let deadline = tokio::time::Instant::now() + PLACEMENT_CONFIRM_WINDOW;
    loop {
        match timeout(
            PLACEMENT_TIMEOUT,
            snapin_platform::place_paster_window(title, x, y),
        )
        .await
        {
            Ok(Ok(true)) => {
                tracing::debug!("贴图窗口已完成精确放置: {title}，坐标: ({x}, {y})");
                return;
            }
            Ok(Ok(false)) => {}
            Ok(Err(error)) => {
                tracing::warn!("确认贴图放置时调用扩展失败: {error}");
                return;
            }
            Err(_) => {
                tracing::warn!("确认贴图放置时调用扩展超时");
                return;
            }
        }

        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(
                "贴图窗口在 {}ms 内仍未能被扩展定位，已回退到合成器默认位置: {title}",
                PLACEMENT_CONFIRM_WINDOW.as_millis()
            );
            return;
        }
        tokio::time::sleep(PLACEMENT_CONFIRM_INTERVAL).await;
    }
}

/// 非 Linux 平台无需 GNOME 扩展确认放置
#[cfg(not(target_os = "linux"))]
pub(crate) async fn confirm_paster_placement(_title: &str, _x: i32, _y: i32) {}
