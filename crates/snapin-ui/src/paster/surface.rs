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

pub use super::alpha_surface::ArgbShmSurface;

use crate::error::UiError;
use std::num::NonZeroU32;
use std::sync::Arc;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 贴图窗口图形表面：优先透明 ARGB8888 共享内存，失败时回退无 alpha 的 softbuffer。
pub enum PasterSurface {
    /// 自建 ARGB8888 `wl_shm` 共享内存表面，支持真正的透明像素合成。
    ///
    /// 装箱保存：该变体内部聚合了事件队列、连接与多个协议代理，直接内联会让整个枚举
    /// 膨胀到数百字节；装箱后枚举本体与 softbuffer 回退通路相当，便于按值传递与匹配。
    Argb(Box<ArgbShmSurface>),
    /// softbuffer 兼容回退通路（`Xrgb8888`，无 alpha），仅在 ARGB 初始化失败时启用
    Softbuffer {
        /// 软渲染上下文，供 `surface` 借用；必须与 `surface` 保持相同生命周期
        _context: softbuffer::Context<Arc<Window>>,
        /// softbuffer 帧缓冲表面
        surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    },
}

impl PasterSurface {
    /// 创建贴图窗口图形表面，优先尝试 ARGB8888 共享内存通路。
    ///
    /// # Errors
    ///
    /// 当 ARGB 通路初始化失败并回退后，softbuffer 上下文或表面创建仍失败时返回
    /// [`UiError::Softbuffer`]（含中文原因描述）。
    pub fn new(window: Arc<Window>) -> Result<Self, UiError> {
        match ArgbShmSurface::new(window.clone()) {
            Ok(surface) => Ok(Self::Argb(Box::new(surface))),
            Err(e) => {
                tracing::warn!(
                    "ARGB8888 共享内存表面初始化失败，已回退 softbuffer 无透明通路（阴影与窗口空白区域将呈现为不透明底色），原因: {e}"
                );
                let context =
                    softbuffer::Context::new(window.clone()).map_err(|e| UiError::Softbuffer {
                        message: format!("初始化贴图软渲染上下文失败: {e}"),
                    })?;
                let surface = softbuffer::Surface::new(&context, window).map_err(|e| {
                    UiError::Softbuffer {
                        message: format!("创建贴图软渲染帧缓冲表面失败: {e}"),
                    }
                })?;
                Ok(Self::Softbuffer {
                    _context: context,
                    surface,
                })
            }
        }
    }

    /// 声明下一帧的目标呈现尺寸（零尺寸收敛为 1 像素）。
    ///
    /// # Errors
    ///
    /// ARGB 通路下尺寸超出 Wayland `i32` 表示范围返回 [`UiError::ShmSurface`]；
    /// softbuffer 通路下尺寸越界返回 [`UiError::Softbuffer`]。
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), UiError> {
        match self {
            Self::Argb(surface) => surface.resize(width, height),
            Self::Softbuffer { surface, .. } => {
                // 零尺寸在窗口最小化等瞬时状态下可能出现，收敛为 1 像素以满足 softbuffer 的非零约束
                let w = NonZeroU32::new(width).unwrap_or(NonZeroU32::MIN);
                let h = NonZeroU32::new(height).unwrap_or(NonZeroU32::MIN);
                surface.resize(w, h).map_err(|e| UiError::Softbuffer {
                    message: format!("调整贴图帧缓冲尺寸失败: {e}"),
                })
            }
        }
    }

    /// 将一帧渲染完成的位图提交给合成器显示。
    ///
    /// # Errors
    ///
    /// 位图尺寸与 [`Self::resize`] 声明的尺寸不一致时返回 [`UiError::Render`]；
    /// 共享内存或 softbuffer 底层操作失败时分别返回 [`UiError::ShmSurface`]、[`UiError::Softbuffer`]。
    pub fn present(&mut self, pixmap: &Pixmap) -> Result<(), UiError> {
        match self {
            Self::Argb(surface) => surface.present(pixmap),
            Self::Softbuffer { surface, .. } => {
                crate::surface::blit_pixmap_to_surface(surface, pixmap)
            }
        }
    }

    /// 声明窗口可交互区域，坐标为 surface-local 逻辑像素。
    ///
    /// 传空切片等价于“整窗不接受任何输入”。透明扩窗后必须由调用方把图像、
    /// 工具条与菜单等可见面板的矩形传入，避免透明空白吞掉桌面点击。
    ///
    /// # Errors
    ///
    /// ARGB 通路下提交 `wl_region` 失败时返回 [`UiError::ShmSurface`]；
    /// softbuffer 回退通路不涉及输入区域，恒返回 `Ok(())`。
    pub fn set_input_rects(&mut self, rects: &[(i32, i32, i32, i32)]) -> Result<(), UiError> {
        match self {
            Self::Argb(surface) => surface.set_input_rects(rects),
            Self::Softbuffer { .. } => {
                // 无透明通路时窗口不参与透明合成，也不存在透明的可点击穿透区域，无需设置输入区域
                Ok(())
            }
        }
    }

    /// 判定当前通路是否支持真正的透明像素合成。
    ///
    /// ARGB8888 共享内存通路（[`Self::Argb`]）的缓冲区带 alpha 通道，未被内容覆盖的像素
    /// 会被合成器真正合成，窗口扩容的空白区域可以保持全透明；softbuffer 回退通路
    /// （[`Self::Softbuffer`]）的 `Xrgb8888` 缓冲区无 alpha 通道，未被覆盖的像素只能呈现黑色。
    /// 渲染层据此决定是否需要为被撑大的空白区域预先铺设不透明底色。
    pub fn supports_transparency(&self) -> bool {
        match self {
            Self::Argb(_) => true,
            Self::Softbuffer { .. } => false,
        }
    }
}
