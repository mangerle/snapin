//! 后台媒体任务契约。

use super::error::PortError;
use super::types::{OcrExportFormat, OcrTextItem, QrCodeItem};
use image::RgbaImage;
use snapin_annotation::shapes::TranslationShapeItem;
use snapin_foundation::beautify::BeautifyOptions;
use snapin_foundation::translation::CoverMode;
use std::path::PathBuf;

/// 后台媒体任务标识。
///
/// 每次提交由执行器分配单调递增 ID，结果回调必须携带同一 ID；窗口据此丢弃
/// 取消后仍可能到达的过期回调，避免旧任务覆盖新任务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(u64);

impl JobId {
    /// 由执行器分配任务序号。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 读取原始序号，供日志与调试展示。
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// 贴图窗口后台任务类别。
///
/// 任务回调只携带一个 [`MediaJobOutcome`]，用任务类别校验结果是否属于发起它的
/// 任务；这样可以同时拒绝过期回调与错配结果，避免把保存路径写进 OCR 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasterJobKind {
    /// 整图文字识别
    Ocr,
    /// 整图原位翻译
    Translate,
    /// 贴图保存
    Save,
}

/// 需要后台执行的一次性媒体任务。
#[derive(Debug)]
pub enum MediaJob {
    /// 对图像执行离线 OCR，返回识别全文。
    Ocr {
        /// 已完成标注合成的选区图像
        image: RgbaImage,
        /// 选区图像左上角在全屏坐标系中的像素原点
        origin: (u32, u32),
    },
    /// 检测并解码图像中的全部二维码。
    DecodeQr {
        /// 已完成标注合成的选区图像
        image: RgbaImage,
        /// 选区图像左上角在全屏坐标系中的像素原点
        origin: (u32, u32),
    },
    /// 对图像执行 OCR 与逐段翻译，返回可直接注入标注历史的结果。
    Translate {
        /// 已完成标注合成的选区图像
        image: RgbaImage,
        /// 图像在全屏坐标系中的像素原点，贴图整图翻译固定为 `(0, 0)`
        origin: (u32, u32),
        /// 原位翻译背景覆盖模式
        cover_mode: CoverMode,
    },
    /// 按美化配置在后台合成最终图像，供复制与贴图前使用。
    ///
    /// 美化渲染包含大画布分配、缩放、圆角遮罩与阴影绘制，必须留在后台线程执行；
    /// 关闭美化时 UI 直接使用原图，不提交本任务。
    Beautify {
        /// 已完成标注合成的选区图像
        image: RgbaImage,
        /// 美化参数
        options: BeautifyOptions,
    },
    /// 保存截图并遵循应用层当前的输出配置。
    SaveScreenshot {
        /// 要保存的最终图像
        image: RgbaImage,
    },
    /// 保存贴图；由应用层决定快捷保存还是弹出系统保存对话框。
    SavePaster {
        /// 要保存的最终图像
        image: RgbaImage,
        /// 是否弹出系统保存对话框
        as_dialog: bool,
        /// 保存成功后是否在文件管理器中打开所在目录
        open_folder_after: bool,
    },
    /// 将用户确认后的 OCR 文本导出为 Markdown 或 JSON。
    SaveOcrText {
        /// 按面板顺序排列的文本行
        lines: Vec<String>,
        /// 目标文本格式
        format: OcrExportFormat,
    },
    /// 渲染美化截图并保存到当前输出目录。
    SaveBeautified {
        /// 已完成标注合成的选区图像
        image: RgbaImage,
        /// 美化参数
        options: BeautifyOptions,
    },
}

/// 媒体任务执行结果。
#[derive(Debug)]
pub enum MediaJobOutcome {
    /// OCR 成功，返回识别全文（可能为空字符串）
    Ocr {
        /// 识别出的完整文本
        text: String,
        /// 按自然阅读顺序排列的文本行
        lines: Vec<OcrTextItem>,
    },
    /// 二维码检测完成，可能返回空集合
    QrCodes {
        /// 检测到的全部二维码
        codes: Vec<QrCodeItem>,
    },
    /// 翻译成功，返回待注入标注历史的翻译图元
    Translated {
        /// 已按原图坐标排版的翻译图元
        items: Vec<TranslationShapeItem>,
    },
    /// 美化合成完成
    Beautified {
        /// 已完成美化渲染的最终图像
        image: RgbaImage,
    },
    /// 保存成功
    Saved {
        /// 最终落盘路径
        path: PathBuf,
    },
    /// 用户在保存对话框中主动取消
    SaveCancelled,
    /// 任务执行失败
    Failed {
        /// 可直接展示的中文错误描述
        message: String,
    },
}

/// 取消句柄。
///
/// 使用装箱闭包而不是绑定 `tokio::task::AbortHandle`，使端口不依赖任何
/// 特定异步运行时；应用层可以在闭包里同时设置取消标志与 abort 底层任务。
pub type JobCancel = Box<dyn Fn() + Send + Sync>;

/// 后台媒体任务执行端口。
///
/// 所有者为提交方：贴图任务在回调闭包中捕获 `PasterId`，截图任务由单会话视图校验
/// `JobId`。执行器因此不需要维护全局 `JobId -> Owner` 映射，避免共享注册表成为
/// 新的生命周期与锁竞争来源；回调只会投递给唯一捕获它的 UI 所有者。
///
/// 实现方必须保证任务在独立线程或运行时中执行，绝不能阻塞调用本方法的窗口事件循环。
/// 回调最多被调用一次；取消是协作式的：实现方应在 OCR、网络翻译、排版与保存阶段之间
/// 检查取消标志并提前返回，丢弃最终结果。ONNX Runtime 单次同步推理无法从外部强制中断，
/// 因此取消只能阻止后续阶段，不能立即释放正在执行的推理线程。
pub trait MediaJobRunner: Send + Sync {
    /// 提交媒体任务并注册结果回调。
    ///
    /// # Errors
    ///
    /// 执行器已停止、队列已满或任务参数非法时返回 [`PortError`]。
    ///
    /// 返回的任务 ID 会原样传给结果回调；取消后执行器可以丢弃结果，但 UI 侧
    /// 仍以 ID 校验兜底，防止竞态回传污染状态。
    fn submit(
        &self,
        job: MediaJob,
        sink: Box<dyn FnOnce(JobId, MediaJobOutcome) + Send>,
    ) -> Result<(JobId, JobCancel), PortError>;
}
