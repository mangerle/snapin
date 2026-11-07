//! 贴图创建与 Wayland 精确放置指令。

use crate::daemon::commands::DaemonResponse;
use crate::placement::{confirm_paster_placement, register_paster_placement};
use snapin_daemon::DaemonErrorCode;
use snapin_foundation::AppPaths;
use snapin_ports::{PasterItem, WorkspaceDriver};
use snapin_runtime::CancellationToken;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tiny_skia::Pixmap;
use tokio::io::AsyncReadExt;

/// 创建贴图窗口所需的完整参数。
pub(super) struct CreatePasterRequest {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) title: String,
    pub(super) raw_path: String,
}

/// 已校验像素数据的贴图投递参数。
struct OpenPasterRequest {
    title: String,
    x: i32,
    y: i32,
    pixmap: Pixmap,
}

/// 一次性像素文件守卫：所有读取失败与任务取消路径都通过析构完成最终清理。
///
/// 显式 [`Self::remove`] 会先尝试异步删除并把错误交给调用方；析构只作为兜底，
/// 因此正常路径可报告清理失败，异常路径也不会遗留运行时临时文件。
struct RuntimePixmapFile {
    path: PathBuf,
    armed: bool,
}

impl RuntimePixmapFile {
    /// 包裹已经通过运行时目录校验的受控临时文件。
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    /// 获取受控文件路径。
    fn path(&self) -> &Path {
        &self.path
    }

    /// 显式删除文件并解除析构兜底；文件已不存在视为清理成功。
    async fn remove(mut self) -> std::io::Result<()> {
        match tokio::fs::remove_file(&self.path).await {
            Ok(()) => {
                self.armed = false;
                Ok(())
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.armed = false;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

impl Drop for RuntimePixmapFile {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                "清理被取消或失败的贴图临时像素文件时出错，路径: {:?}，原因: {error}",
                self.path
            ),
        }
    }
}

/// 创建贴图窗口；返回 `None` 表示在产生 GUI 或系统副作用前已取消。
pub(super) async fn create(
    workspace: &dyn WorkspaceDriver,
    request: CreatePasterRequest,
    cancel: &CancellationToken,
) -> Option<DaemonResponse> {
    if cancel.is_cancelled() {
        return None;
    }
    match prepare_paster_request(request, cancel).await {
        Ok(request) => open_paster(workspace, request).await,
        Err(Some(response)) => Some(response),
        Err(None) => None,
    }
}

/// 校验并读取贴图请求，生成可直接投递到工作区的参数。
async fn prepare_paster_request(
    request: CreatePasterRequest,
    cancel: &CancellationToken,
) -> Result<OpenPasterRequest, Option<DaemonResponse>> {
    let CreatePasterRequest {
        width,
        height,
        x,
        y,
        title,
        raw_path,
    } = request;
    validate_pixel_size(width, height).map_err(Some)?;
    let file_path = PathBuf::from(&raw_path);
    let pixmap = load_runtime_pixmap(file_path, width, height, cancel).await?;
    if cancel.is_cancelled() {
        return Err(None);
    }
    Ok(OpenPasterRequest {
        title,
        x,
        y,
        pixmap,
    })
}

/// 校验像素尺寸是否可安全分配。
fn validate_pixel_size(width: u32, height: u32) -> Result<usize, DaemonResponse> {
    snapin_storage::checked_rgba_byte_len(width, height).ok_or_else(|| {
        DaemonResponse::error(DaemonErrorCode::InvalidInput, "贴图像素尺寸非法或超过上限")
    })
}

/// 校验受控路径、读取像素并在所有失败/取消路径清理临时文件。
async fn load_runtime_pixmap(
    file_path: PathBuf,
    width: u32,
    height: u32,
    cancel: &CancellationToken,
) -> Result<Pixmap, Option<DaemonResponse>> {
    let expected_len = validate_pixel_size(width, height).map_err(Some)?;
    verify_runtime_file(&file_path).await.map_err(Some)?;
    if cancel.is_cancelled() {
        return Err(None);
    }
    let temp_file = RuntimePixmapFile::new(file_path);
    let pixmap = read_pixel_pixmap(temp_file.path(), width, height, expected_len)
        .await
        .map_err(Some)?;
    if cancel.is_cancelled() {
        return Err(None);
    }
    if let Err(error) = temp_file.remove().await {
        return Err(Some(DaemonResponse::error(
            DaemonErrorCode::Io,
            format!("清理贴图临时像素文件失败: {error}"),
        )));
    }
    if cancel.is_cancelled() {
        return Err(None);
    }
    Ok(pixmap)
}

/// 将已读取的贴图投递到工作区并确认 Wayland 精确放置。
async fn open_paster(
    workspace: &dyn WorkspaceDriver,
    request: OpenPasterRequest,
) -> Option<DaemonResponse> {
    let OpenPasterRequest {
        title,
        x,
        y,
        pixmap,
    } = request;
    let uid = AppPaths::generate_unique_id();
    register_paster_placement(&title, x, y).await;
    let item = PasterItem::new(uid, title.clone(), pixmap, (x, y));
    if let Err(error) = workspace.open_paster(item) {
        tracing::warn!("在集中工作区中打开贴图失败: {error}");
        return Some(DaemonResponse::error(
            DaemonErrorCode::Unavailable,
            format!("打开贴图失败: {error}"),
        ));
    }
    // 窗口已投递到工作区线程，此时再轮询确认放置，顺序不能提前。
    confirm_paster_placement(&title, x, y).await;
    Some(DaemonResponse::Ok)
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
        Err(DaemonResponse::error(
            DaemonErrorCode::InvalidInput,
            "非法贴图临时物理路径",
        ))
    }
}

/// 读取一次性像素文件，直接把像素流入预分配位图，避免整文件中间缓冲。
///
/// 清理由调用方的 [`RuntimePixmapFile`] 负责，确保读取、分配或取消失败时都不会
/// 遗留受控运行时文件。
async fn read_pixel_pixmap(
    file_path: &Path,
    width: u32,
    height: u32,
    expected_len: usize,
) -> Result<Pixmap, DaemonResponse> {
    let metadata_matches = tokio::fs::metadata(file_path)
        .await
        .map(|metadata| metadata.len() == expected_len as u64)
        .unwrap_or(false);
    if !metadata_matches {
        return Err(DaemonResponse::error(
            DaemonErrorCode::InvalidInput,
            "像素数据长度与图像分辨率不匹配",
        ));
    }

    let Some(mut pixmap) = Pixmap::new(width, height) else {
        return Err(DaemonResponse::error(
            DaemonErrorCode::Internal,
            "分配像素缓冲区失败",
        ));
    };
    let read_result = async {
        let mut file = tokio::fs::File::open(file_path)
            .await
            .map_err(|error| format!("打开贴图临时文件失败: {error}"))?;
        file.read_exact(pixmap.data_mut())
            .await
            .map_err(|error| format!("读取贴图像素数据失败: {error}"))?;
        Ok::<(), String>(())
    }
    .await;
    match read_result {
        Ok(()) => Ok(pixmap),
        Err(message) => Err(DaemonResponse::error(DaemonErrorCode::Io, message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 像素文件应直接流入预分配位图，并在守卫显式清理后删除。
    #[tokio::test]
    async fn test_read_pixel_pixmap_streams_and_removes_file() {
        let file = tempfile::NamedTempFile::new().expect("应可创建临时测试文件");
        tokio::fs::write(file.path(), [1u8, 2, 3, 255])
            .await
            .expect("应可写入测试像素");
        let guard = RuntimePixmapFile::new(file.path().to_path_buf());
        let pixmap = read_pixel_pixmap(guard.path(), 1, 1, 4)
            .await
            .expect("应可读取 1x1 像素");
        assert_eq!(pixmap.data(), &[1, 2, 3, 255]);
        guard.remove().await.expect("显式清理像素文件失败");
        assert!(!file.path().exists(), "读取成功后必须回收临时文件");
    }

    /// 任务在读取途中被取消时，守卫析构仍必须删除一次性像素文件。
    #[test]
    fn test_runtime_pixmap_file_removes_on_drop() {
        let file = tempfile::NamedTempFile::new().expect("应可创建临时测试文件");
        let path = file.path().to_path_buf();
        drop(RuntimePixmapFile::new(path.clone()));
        assert!(!path.exists(), "守卫析构必须回收临时像素文件");
    }

    /// 运行时目录外的任意文件不得通过贴图 IPC 读取，防止路径穿越。
    #[tokio::test]
    async fn test_verify_runtime_file_rejects_outside_path() {
        let file = tempfile::NamedTempFile::new().expect("应可创建临时测试文件");
        let result = verify_runtime_file(file.path()).await;
        assert!(matches!(result, Err(DaemonResponse::Error(_))));
    }
}
