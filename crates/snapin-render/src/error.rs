//! 渲染层强类型错误。

use thiserror::Error;

/// 文本排版与美化渲染失败原因。
///
/// 渲染 crate 不依赖配置、平台或 UI 类型，错误只描述绘制阶段本身的可分支场景；
/// 调用方可以据此决定提示文案、降级策略或日志级别，而不必解析中文字符串。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RenderError {
    /// 输入图像宽或高为零
    #[error("美化输入图像尺寸不能为零")]
    EmptyInput,

    /// 计算出的输出画布尺寸非法
    #[error("美化输出尺寸非法，宽度: {width}，高度: {height}")]
    InvalidCanvasSize {
        /// 计算得到的画布宽度
        width: u32,
        /// 计算得到的画布高度
        height: u32,
    },

    /// 输出画布像素缓冲分配失败
    #[error("分配美化输出缓冲区失败，宽度: {width}，高度: {height}")]
    CanvasAllocation {
        /// 画布宽度
        width: u32,
        /// 画布高度
        height: u32,
    },

    /// 缩放后的内容缓冲分配失败
    #[error("分配美化缩放缓冲区失败，宽度: {width}，高度: {height}")]
    ScaledAllocation {
        /// 内容区宽度
        width: u32,
        /// 内容区高度
        height: u32,
    },

    /// 圆角遮罩像素缓冲分配失败
    #[error("分配美化圆角遮罩失败，宽度: {width}，高度: {height}")]
    MaskAllocation {
        /// 遮罩宽度
        width: u32,
        /// 遮罩高度
        height: u32,
    },

    /// 圆角矩形路径构造失败
    #[error("构造美化圆角遮罩路径失败，宽度: {width}，高度: {height}，圆角: {radius}")]
    MaskPath {
        /// 内容区宽度
        width: f32,
        /// 内容区高度
        height: f32,
        /// 圆角半径
        radius: f32,
    },

    /// 背景渐变着色器构造失败
    #[error("构造美化背景渐变失败")]
    BackgroundGradient,

    /// 背景覆盖矩形构造失败
    #[error("构造美化背景矩形失败，宽度: {width}，高度: {height}")]
    BackgroundRect {
        /// 画布宽度
        width: u32,
        /// 画布高度
        height: u32,
    },
}
