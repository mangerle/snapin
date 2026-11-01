//! 贴图窗口图形表面抽象与后端分发。
//!
//! # 为什么需要这一层
//!
//! 贴图窗口需要真正支持透明像素（右键菜单撑大窗口后的空白区域、贴图半透明阴影与圆角边框）。
//! 而 softbuffer 0.4.8 的 Wayland 后端把 `wl_shm` 缓冲区格式硬编码为 `Xrgb8888`（无 alpha 通道），
//! 透明像素只能被渲染成黑色，因此这里优先使用自建的 ARGB8888 共享内存通路（见 [`alpha_surface`]），
//! 仅在初始化失败（非 Wayland 后端、`wl_shm` 缺失等）时回退到 softbuffer 无透明通路，
//! 保证贴图窗口在异常环境下仍可显示，只是失去透明合成能力。
//!
//! # 回退策略
//!
//! 回退只发生在 **[`PasterSurface::new`] 初始化阶段**：一旦选定后端，后续 `resize`/`present`
//! 不再切换通路，避免逐帧切换带来的状态分裂。回退时会输出一条中文 warn 日志说明原因，
//! 便于线上排查透明效果缺失的根因。

#[cfg(target_os = "linux")]
pub use super::alpha_surface::ArgbShmSurface;

use crate::error::UiError;
use std::num::NonZeroU32;
use std::sync::Arc;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 贴图窗口图形表面：Linux Wayland 优先透明 ARGB8888 共享内存，失败或其它操作系统回退 softbuffer。
pub enum PasterSurface {
    /// 自建 ARGB8888 `wl_shm` 共享内存表面，支持真正的透明像素合成（仅 Linux Wayland）。
    #[cfg(target_os = "linux")]
    Argb(Box<ArgbShmSurface>),
    /// softbuffer 跨平台兼容通路（`Xrgb8888`，无 alpha）
    Softbuffer {
        /// 软渲染上下文，供 `surface` 借用；必须与 `surface` 保持相同生命周期
        _context: softbuffer::Context<Arc<Window>>,
        /// softbuffer 帧缓冲表面
        surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    },
}

impl PasterSurface {
    /// 创建贴图窗口图形表面，Linux Wayland 优先尝试 ARGB8888 共享内存通路。
    pub fn new(window: Arc<Window>) -> Result<Self, UiError> {
        #[cfg(target_os = "linux")]
        {
            match ArgbShmSurface::new(window.clone()) {
                Ok(surface) => return Ok(Self::Argb(Box::new(surface))),
                Err(e) => {
                    tracing::warn!(
                        "ARGB8888 共享内存表面初始化失败，已回退 softbuffer 无透明通路，原因: {e}"
                    );
                }
            }
        }

        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| UiError::Softbuffer {
                message: format!("初始化贴图软渲染上下文失败: {e}"),
            })?;
        let surface =
            softbuffer::Surface::new(&context, window).map_err(|e| UiError::Softbuffer {
                message: format!("创建贴图软渲染帧缓冲表面失败: {e}"),
            })?;
        Ok(Self::Softbuffer {
            _context: context,
            surface,
        })
    }

    /// 声明下一帧的目标呈现尺寸（零尺寸收敛为 1 像素）。
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), UiError> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Argb(surface) => surface.resize(width, height),
            Self::Softbuffer { surface, .. } => {
                let w = NonZeroU32::new(width).unwrap_or(NonZeroU32::MIN);
                let h = NonZeroU32::new(height).unwrap_or(NonZeroU32::MIN);
                surface.resize(w, h).map_err(|e| UiError::Softbuffer {
                    message: format!("调整贴图帧缓冲尺寸失败: {e}"),
                })
            }
        }
    }

    /// 将一帧渲染完成的位图提交给合成器显示。
    pub fn present(&mut self, pixmap: &Pixmap) -> Result<(), UiError> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Argb(surface) => surface.present(pixmap),
            Self::Softbuffer { surface, .. } => {
                crate::surface::blit_pixmap_to_surface(surface, pixmap)
            }
        }
    }

    /// 声明窗口可交互区域，坐标为 surface-local 逻辑像素。
    pub fn set_input_rects(&mut self, rects: &[(i32, i32, i32, i32)]) -> Result<(), UiError> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Argb(surface) => surface.set_input_rects(rects),
            Self::Softbuffer { .. } => {
                let _ = rects;
                Ok(())
            }
        }
    }

    /// 判定当前通路是否支持真正的透明像素合成。
    pub fn supports_transparency(&self) -> bool {
        match self {
            #[cfg(target_os = "linux")]
            Self::Argb(_) => true,
            Self::Softbuffer { .. } => false,
        }
    }
}
