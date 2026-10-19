//! softbuffer 共享内存帧缓冲呈现与格式转换基础设施。
//!
//! 提供将 tiny-skia 的 RGBA 像素高速 Blit 到操作系统原生 softbuffer 0RGB 表面呈现的通用接口。

use crate::error::SnapinError;
use std::sync::Arc;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 将 tiny-skia 像素高速复制到 softbuffer 共享内存缓冲区并提交显示
pub fn blit_pixmap_to_surface(
    surface: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    pixmap: &Pixmap,
) -> Result<(), SnapinError> {
    let mut buffer = surface
        .buffer_mut()
        .map_err(|e| SnapinError::SoftbufferError {
            message: format!("获取绘图表面缓冲区失败: {e}"),
        })?;

    let src_bytes = pixmap.data();
    for (dest, src) in buffer.iter_mut().zip(src_bytes.as_chunks::<4>().0) {
        *dest = ((src[0] as u32) << 16) | ((src[1] as u32) << 8) | (src[2] as u32);
    }

    buffer.present().map_err(|e| SnapinError::SoftbufferError {
        message: format!("帧缓冲提交显示失败: {e}"),
    })?;

    Ok(())
}
