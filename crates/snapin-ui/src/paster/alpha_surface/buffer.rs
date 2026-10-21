//! 贴图 ARGB8888 共享内存缓冲区及其像素打包、尺寸校验与提交辅助实现。

use super::ShmState;
use crate::error::UiError;
use memmap2::MmapMut;
use std::fs::File;
use std::os::fd::AsFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tiny_skia::Pixmap;
use wayland_client::protocol::{wl_buffer, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Proxy, QueueHandle};

/// 将单个 tiny-skia 预乘 RGBA 像素打包为 `wl_shm::Format::Argb8888` 所需的 32 位字。
///
/// 小端序下 `Argb8888` 的内存字节序为 B、G、R、A，对应 32 位字的位布局 `a<<24 | r<<16 | g<<8 | b`。
/// tiny-skia 的 `Pixmap` 与 `Argb8888` 均要求预乘 alpha，因此无需做非预乘转换，逐字节重排即可。
/// 该公式与 snipper 的 softbuffer 通路（`crate::surface::blit_pixmap_to_surface`）完全一致，
/// 保证两条通路输出的像素逐位相同。
pub(super) fn pack_premultiplied_rgba(pixel: [u8; 4]) -> u32 {
    ((pixel[3] as u32) << 24)
        | ((pixel[0] as u32) << 16)
        | ((pixel[1] as u32) << 8)
        | (pixel[2] as u32)
}

/// 将 tiny-skia 位图的预乘 RGBA 像素逐点打包写入共享内存映射。
///
/// 调用方必须保证映射长度与位图像素数一致（[`ArgbShmSurface::present`] 已前置校验），
/// 这里按迭代器逐点写入，长度不一致时以较短一方为准，绝不越界。
pub(super) fn copy_pixmap_into(dest: &mut [u32], pixmap: &Pixmap) {
    for (slot, src) in dest.iter_mut().zip(pixmap.data().as_chunks::<4>().0) {
        *slot = pack_premultiplied_rgba(*src);
    }
}

/// 计算 ARGB8888 缓冲区所需字节数，全程使用检查算术，防止异常窗口尺寸引发算数溢出。
pub(super) fn buffer_byte_len(width: i32, height: i32) -> Result<i32, UiError> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|len| *len > 0)
        .ok_or_else(|| UiError::ShmSurface {
            message: format!("共享内存缓冲区尺寸非法，宽度: {width}，高度: {height}"),
        })
}

/// 将像素尺寸（u32）转换为 Wayland `wl_shm` 要求的 i32 尺寸。
pub(super) fn checked_wayland_size(width: u32, height: u32) -> Result<(i32, i32), UiError> {
    let w = i32::try_from(width).map_err(|_| size_overflow_error(width, height))?;
    let h = i32::try_from(height).map_err(|_| size_overflow_error(width, height))?;
    Ok((w, h))
}

/// 构造尺寸越界的中文错误（宽度、高度同时给出，便于定位实际取到的异常尺寸）。
fn size_overflow_error(width: u32, height: u32) -> UiError {
    UiError::ShmSurface {
        message: format!("贴图表面尺寸超出 Wayland 表示范围，宽度: {width}，高度: {height}"),
    }
}

/// 对整幅缓冲区施加 damage 并 commit。
///
/// v4 以下版本的 `wl_surface` 没有 `damage_buffer`，照 Mesa EGL/WSI 的兼容做法
/// 使用 `damage(0, 0, i32::MAX, i32::MAX)` 声明全量重绘；v4 及以上按实际缓冲区尺寸精确标注，
/// 减少合成器的无效重绘区域。
pub(super) fn damage_and_commit(surface: &wl_surface::WlSurface, width: i32, height: i32) {
    if surface.version() < 4 {
        surface.damage(0, 0, i32::MAX, i32::MAX);
    } else {
        surface.damage_buffer(0, 0, width, height);
    }
    surface.commit();
}

/// 单块 ARGB8888 共享内存缓冲区及其配套资源。
///
/// RAII 释放：析构本结构体即发送 `wl_buffer` 与 `wl_shm_pool` 的销毁请求，
/// 并随 `File` 关闭回收 memfd、随 `MmapMut` 析构解除映射（销毁请求的冲刷由所属表面统一负责）。
pub(super) struct ShmBuffer {
    /// wl_shm 内存池代理
    pool: wl_shm_pool::WlShmPool,
    /// wl_buffer 代理
    buffer: wl_buffer::WlBuffer,
    /// memfd 的 mmap 映射
    map: MmapMut,
    /// memfd 文件描述符（保留到析构，服务端在创建池时已复制该 fd）
    _file: File,
    /// 缓冲区宽度（像素）
    pub(super) width: i32,
    /// 缓冲区高度（像素）
    pub(super) height: i32,
    /// 合成器是否已归还本缓冲区（收到 `wl_buffer.release` 后置真）
    released: Arc<AtomicBool>,
}

impl ShmBuffer {
    /// 创建 memfd 共享内存并注册对应的 `wl_shm_pool` 与 `wl_buffer`（格式固定 `Argb8888`）。
    pub(super) fn new(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<ShmState>,
        width: i32,
        height: i32,
    ) -> Result<Self, UiError> {
        let byte_len = buffer_byte_len(width, height)?;

        let fd = rustix::fs::memfd_create(
            c"snapin-argb-shm",
            rustix::fs::MemfdFlags::CLOEXEC | rustix::fs::MemfdFlags::ALLOW_SEALING,
        )
        .map_err(|e| UiError::ShmSurface {
            message: format!("创建共享内存 memfd 失败，长度: {byte_len} 字节，原因: {e}"),
        })?;
        // 封存 F_SEAL_SHRINK 防止缓冲区被意外收缩（合成器按注册时的固定尺寸读取共享内存），
        // 封存 F_SEAL_SEAL 之后不再允许追加其它封存标志
        rustix::fs::fcntl_add_seals(
            &fd,
            rustix::fs::SealFlags::SHRINK | rustix::fs::SealFlags::SEAL,
        )
        .map_err(|e| UiError::ShmSurface {
            message: format!("为共享内存 memfd 添加封存标志失败，原因: {e}"),
        })?;
        let file = File::from(fd);
        file.set_len(byte_len as u64)
            .map_err(|e| UiError::ShmSurface {
                message: format!("设置共享内存长度失败，长度: {byte_len} 字节，原因: {e}"),
            })?;

        // SAFETY: `MmapMut::map_mut` 要求调用方保证映射存续期内文件不会被并发修改长度。
        // 此处满足该前提：memfd 已封存 F_SEAL_SHRINK 与 F_SEAL_SEAL，只有本结构体持有其
        // 文件描述符与映射，长度在整个缓冲区生命周期内恒定不变。
        let map = unsafe { MmapMut::map_mut(&file) }.map_err(|e| UiError::ShmSurface {
            message: format!("映射共享内存失败，长度: {byte_len} 字节，原因: {e}"),
        })?;

        let pool = shm.create_pool(file.as_fd(), byte_len, qh, ());
        let released = Arc::new(AtomicBool::new(true));
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            width * 4,
            wl_shm::Format::Argb8888,
            qh,
            Arc::clone(&released),
        );

        Ok(Self {
            pool,
            buffer,
            map,
            _file: file,
            width,
            height,
            released,
        })
    }

    /// 判断缓冲区尺寸是否与目标尺寸一致（不一致时需整体重建）。
    pub(super) fn matches(&self, width: i32, height: i32) -> bool {
        self.width == width && self.height == height
    }

    /// 合成器是否已归还本缓冲区（归还后其内存可安全覆写）。
    pub(super) fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }

    /// 将本缓冲区挂载到表面显示。
    pub(super) fn attach(&self, surface: &wl_surface::WlSurface) {
        // 先置占用位再 attach：若在 attach 之后置位，恰好在该间隙到达的 Release 事件会被
        // 误判为“已被归还”，导致后续帧写入合成器正在读取的内存
        self.released.store(false, Ordering::Release);
        surface.attach(Some(&self.buffer), 0, 0);
    }

    /// 以 32 位字视图访问映射的像素内存。
    pub(super) fn pixels_mut(&mut self) -> &mut [u32] {
        let len = self.width as usize * self.height as usize;
        // SAFETY: mmap 返回的基址按页对齐（满足 u32 的对齐要求）；缓冲区字节长度恒为
        // width*height*4 且已在创建时通过检查算术校验不溢出，因此 len 个 u32 完整落在映射区间内；
        // 映射随 `map` 字段存活，此处借出的 &mut 保证借用期内映射不会被释放或重映射。
        unsafe { std::slice::from_raw_parts_mut(self.map.as_mut_ptr().cast::<u32>(), len) }
    }
}

impl Drop for ShmBuffer {
    fn drop(&mut self) {
        // wayland-client 的代理仅持有对象句柄，drop 不会自动发送析构请求，必须显式销毁，
        // 否则服务端会一直持有 wl_buffer 与 wl_shm_pool（连同其复制的 memfd）直至连接关闭
        self.buffer.destroy();
        self.pool.destroy();
    }
}
