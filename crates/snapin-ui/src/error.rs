//! Snapin UI 视图与窗口渲染层领域错误模型。
//!
//! 封装 Wayland 窗口生命周期、事件循环初始化、softbuffer 共享内存帧缓冲操作
//! 以及像素内存缓冲分配异常。

use snapin_core::error::CoreError;
use thiserror::Error;

/// UI 视图与窗口渲染层领域错误枚举
#[derive(Error, Debug)]
pub enum UiError {
    /// Wayland 或底层操作系统窗口与事件循环异常
    #[error("Wayland 窗口系统异常，描述: {message}")]
    Window { message: String },

    /// 帧缓冲分配与渲染呈现异常
    #[error("softbuffer 共享内存帧缓冲操作失败，描述: {message}")]
    Softbuffer { message: String },

    /// Pixmap 像素内存缓冲分配失败
    #[error("分配像素内存缓冲失败，宽度: {width}，高度: {height}")]
    PixmapAlloc { width: u32, height: u32 },

    /// 目标帧缓冲像素容量与源位图不一致，通常源于窗口尺寸变更与重绘之间的竞态
    #[error("渲染呈现失败，描述: {message}")]
    Render { message: String },

    /// 核心领域错误透明转换
    #[error(transparent)]
    Core(#[from] CoreError),
}
