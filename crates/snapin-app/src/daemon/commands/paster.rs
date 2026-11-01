//! 贴图创建与 Wayland 精确放置指令。

use crate::daemon::commands::DaemonResponse;
use snapin_core::AppPaths;
use snapin_ui::paster::{PasterItem, PasterWorkspaceProxy};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tiny_skia::Pixmap;
use tokio::time::timeout;

#[cfg(target_os = "linux")]
const PLACEMENT_TIMEOUT: Duration = Duration::from_millis(400);
#[cfg(target_os = "linux")]
const PLACEMENT_CONFIRM_WINDOW: Duration = Duration::from_millis(800);
#[cfg(target_os = "linux")]
const PLACEMENT_CONFIRM_INTERVAL: Duration = Duration::from_millis(40);

/// 向配套 GNOME 扩展登记贴图窗口的精确放置坐标。
///
/// 设计原理：Wayland 的 xdg-shell 协议禁止客户端自行决定窗口位置，`WindowBuilder::with_position`
/// 在原生会话下被合成器直接忽略，绝对定位只能交由持有 Mutter 特权的扩展执行。
/// 这里采用「先登记坐标、再创建窗口」的次序，扩展会把尚未映射的请求挂起，
/// 待 `window-created` 触发时立即生效，从而彻底消除窗口映射与放置调用之间的竞态。
/// 放置失败只影响窗口落点、不影响贴图内容本身，属可降级路径，故仅记录日志而不改写响应结果。
#[cfg(target_os = "linux")]
async fn register_paster_placement(title: &str, x: i32, y: i32) {
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
async fn register_paster_placement(_title: &str, _x: i32, _y: i32) {}

/// 在窗口投递之后确认放置已真正生效（仅 Linux Wayland 生效）
#[cfg(target_os = "linux")]
async fn confirm_paster_placement(title: &str, x: i32, y: i32) {
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
async fn confirm_paster_placement(_title: &str, _x: i32, _y: i32) {}

/// 创建贴图窗口所需的完整参数。
pub(super) struct CreatePasterRequest {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) title: String,
    pub(super) raw_path: String,
}

pub(super) async fn create(
    workspace: &PasterWorkspaceProxy,
    request: CreatePasterRequest,
) -> DaemonResponse {
    let CreatePasterRequest {
        width,
        height,
        x,
        y,
        title,
        raw_path,
    } = request;
    let Some(expected_len) = snapin_storage::checked_rgba_byte_len(width, height) else {
        return DaemonResponse::Error("贴图像素尺寸非法或超过上限".to_string());
    };
    let file_path = PathBuf::from(&raw_path);

    if let Err(response) = verify_runtime_file(&file_path).await {
        return response;
    }
    let bytes = match read_pixel_bytes(&file_path, expected_len).await {
        Ok(bytes) => bytes,
        Err(response) => return response,
    };
    let pixmap = match build_pixmap(width, height, &bytes) {
        Ok(pixmap) => pixmap,
        Err(response) => return response,
    };

    let uid = AppPaths::generate_unique_id();
    register_paster_placement(&title, x, y).await;
    let item = PasterItem::new(uid, title.clone(), pixmap, (x, y));
    if let Err(error) = workspace.open_paster(item) {
        tracing::error!("在集中工作区中打开贴图失败: {error}");
        return DaemonResponse::Error(format!("打开贴图失败: {error}"));
    }
    // 窗口已投递到工作区线程，此时再轮询确认放置，顺序不能提前
    confirm_paster_placement(&title, x, y).await;
    DaemonResponse::Ok
}

/// 校验临时像素文件位于受控运行时目录内，阻断任意路径读取。
async fn verify_runtime_file(file_path: &Path) -> Result<(), DaemonResponse> {
    let runtime_dir = AppPaths::runtime_dir();
    let runtime_dir = runtime_dir
        .canonicalize()
        .unwrap_or_else(|_| runtime_dir.clone());
    let path = file_path.to_path_buf();
    let is_safe = tokio::task::spawn_blocking(move || {
        path.canonicalize()
            .map(|path| path.starts_with(&runtime_dir))
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false);

    if is_safe {
        Ok(())
    } else {
        tracing::warn!("拒绝接收不受信任目录下的贴图临时文件: {:?}", file_path);
        Err(DaemonResponse::Error("非法贴图临时物理路径".to_string()))
    }
}

/// 读取并回收一次性像素文件，要求实际长度与声明分辨率严格一致。
async fn read_pixel_bytes(
    file_path: &Path,
    expected_len: usize,
) -> Result<Vec<u8>, DaemonResponse> {
    let metadata_matches = tokio::fs::metadata(file_path)
        .await
        .map(|metadata| metadata.len() == expected_len as u64)
        .unwrap_or(false);
    if !metadata_matches {
        return Err(DaemonResponse::Error(
            "像素数据长度与图像分辨率不匹配".to_string(),
        ));
    }

    let bytes = match tokio::fs::read(file_path).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!("读取贴图临时文件失败: {error}");
            return Err(DaemonResponse::Error(format!(
                "读取贴图临时文件失败: {error}"
            )));
        }
    };
    let _ = tokio::fs::remove_file(file_path).await;
    if bytes.len() != expected_len {
        return Err(DaemonResponse::Error(
            "像素数据长度与图像分辨率不匹配".to_string(),
        ));
    }
    Ok(bytes)
}

/// 将校验后的 RGBA 字节复制进 tiny-skia 位图。
fn build_pixmap(width: u32, height: u32, bytes: &[u8]) -> Result<Pixmap, DaemonResponse> {
    let Some(mut pixmap) = Pixmap::new(width, height) else {
        return Err(DaemonResponse::Error("分配像素缓冲区失败".to_string()));
    };
    pixmap.data_mut().copy_from_slice(bytes);
    Ok(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构建位图后必须保留 RGBA 原始字节，避免尺寸与内容错位。
    #[test]
    fn test_build_pixmap_copies_rgba_bytes() {
        let pixmap = build_pixmap(1, 1, &[1, 2, 3, 255]).expect("1x1 RGBA 位图应可分配");
        assert_eq!(pixmap.data(), &[1, 2, 3, 255]);
    }

    /// 运行时目录外的任意文件不得通过贴图 IPC 读取，防止路径穿越。
    #[tokio::test]
    async fn test_verify_runtime_file_rejects_outside_path() {
        let file = tempfile::NamedTempFile::new().expect("应可创建临时测试文件");
        let result = verify_runtime_file(file.path()).await;
        assert!(matches!(result, Err(DaemonResponse::Error(_))));
    }
}
