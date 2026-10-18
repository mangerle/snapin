//! 贴图变换光栅化合成与软帧缓冲提交。

use crate::error::SnapinError;
use crate::paster::manager::PasterItem;
use std::num::NonZeroU32;
use std::sync::Arc;
use tiny_skia::{Pixmap, PixmapPaint, Transform};
use winit::window::Window;

/// 根据几何变换矩阵将贴图图元光栅化到目标缓冲区并推送到 softbuffer
pub fn render_paster_to_surface(
    item: &PasterItem,
    surface: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    width: u32,
    height: u32,
) -> Result<(), SnapinError> {
    let w = NonZeroU32::new(width.max(1)).ok_or_else(|| SnapinError::SoftbufferError {
        message: "无效的贴图表面宽度".to_string(),
    })?;
    let h = NonZeroU32::new(height.max(1)).ok_or_else(|| SnapinError::SoftbufferError {
        message: "无效的贴图表面高度".to_string(),
    })?;

    surface
        .resize(w, h)
        .map_err(|e| SnapinError::SoftbufferError {
            message: format!("调整贴图帧缓冲尺寸失败: {e}"),
        })?;

    let mut target_pixmap =
        Pixmap::new(width, height).ok_or_else(|| SnapinError::SoftbufferError {
            message: "分配贴图渲染双缓冲区失败".to_string(),
        })?;

    // 1. 构建仿射几何变换矩阵 (缩放、翻转、旋转)
    let scale = item.transform.scale;
    let mut tf = Transform::from_scale(scale, scale);

    if item.transform.flip_horizontal {
        tf = tf.pre_scale(-1.0, 1.0);
    }
    if item.transform.flip_vertical {
        tf = tf.pre_scale(1.0, -1.0);
    }

    // 2. 配置不透明度
    let paint = PixmapPaint {
        opacity: item.transform.opacity,
        ..Default::default()
    };

    target_pixmap.draw_pixmap(0, 0, item.pixmap.as_ref(), &paint, tf, None);

    // 3. 提交到 softbuffer
    let mut buffer = surface
        .buffer_mut()
        .map_err(|e| SnapinError::SoftbufferError {
            message: format!("获取绘图表面缓冲区失败: {e}"),
        })?;

    for (dest, chunk) in buffer
        .iter_mut()
        .zip(target_pixmap.data().as_chunks::<4>().0)
    {
        let r = chunk[0] as u32;
        let g = chunk[1] as u32;
        let b = chunk[2] as u32;
        *dest = (r << 16) | (g << 8) | b;
    }

    buffer.present().map_err(|e| SnapinError::SoftbufferError {
        message: format!("贴图帧缓冲呈现失败: {e}"),
    })?;

    Ok(())
}
