//! 长截图拼接领域模型、选项与几何视口类型定义。

use image::RgbaImage;

/// 手动裁剪允许保留的最小主轴有效像素长度。
///
/// 之所以设为较小的 `32px` 而非锁定在初始单帧尺寸 `initial_frame_size` 上，
/// 是为了允许用户剪掉首帧或单屏截取内容自带的冗余页眉/页脚或多余边距；
/// 当初始选区本身小于 `32px` 时会按初始尺寸夹取，杜绝零面积空图像。
pub const MIN_CROPPED_SPAN: u32 = 32;

/// 侧边预览缩略图的默认最小可读交叉轴厚度（逻辑像素对应的目标像素）。
pub const DEFAULT_MIN_THUMBNAIL_CROSS: u32 = 56;

/// 截图滚动与拼接的主方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StitchDirection {
    /// 垂直方向拼接（从上往下滚动，默认）
    #[default]
    Vertical,
    /// 水平方向拼接（从左往右滚动）
    Horizontal,
}

/// 长截图拼接配置参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StitchOptions {
    /// 边距保护比例，用于剔除左右两侧滚动条与边框杂色干扰（例如 0.10 表示左右各剔除 10% 区域）
    pub margin_ratio: f32,
    /// 单帧允许的最大有效移动位移量（像素），超出此范围视为快速翻页或场景切换，予以保护丢弃
    pub max_step: u32,
    /// 判定为发生内容移动的最小像素阈值，低于此值视为静止
    pub min_step: u32,
    /// 匹配置信度判定阈值 (0.0 ~ 1.0)，低于此值认为未能有效匹配，不追加画布
    pub confidence_threshold: f32,
    /// 是否开启反向滚动自动裁剪（检测到用户反向滚动鼠标时，自动裁切掉长图尾部回滚的内容）
    pub auto_crop_reverse: bool,
    /// 画面顶部（或左侧）固定标题栏/导航条的最大判定比例（默认 0.35）
    pub sticky_header_max_ratio: f32,
}

impl Default for StitchOptions {
    fn default() -> Self {
        Self {
            margin_ratio: 0.10,
            max_step: 300,
            min_step: 2,
            confidence_threshold: 0.70,
            auto_crop_reverse: true,
            sticky_header_max_ratio: 0.35,
        }
    }
}

/// 连续帧之间的位移匹配分析成果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchResult {
    /// 沿主方向的位移量（像素）。
    /// 正数表示正常向前滚动（内容向下/向右推进）；
    /// 负数表示反向回滚（内容向上/向左回退）；
    /// 0 表示画面未产生位移或特征不足以匹配。
    pub offset: i32,
    /// 匹配置信度得分（0.0 ~ 1.0）
    pub confidence: f32,
    /// 识别出的固定顶栏（或固定侧边栏）厚度（像素）
    pub sticky_header_size: u32,
}

impl MatchResult {
    /// 静态未移动或匹配失败结果
    pub const fn idle() -> Self {
        Self {
            offset: 0,
            confidence: 0.0,
            sticky_header_size: 0,
        }
    }

    /// 当前帧是否成功匹配且产生了有效移动
    pub fn is_matched(&self, threshold: f32) -> bool {
        self.offset != 0 && self.confidence >= threshold
    }
}

/// 单帧图像投喂给长截图拼接器后的真实状态转移结果。
///
/// 实现初衷：仅仅返回 `MatchResult` 无法区分“画面静止无变化”、“匹配失败未拼接”、
/// “尺寸/回滚设置被忽略”以及“正向拼入或反向裁剪确实更新了画布与参考帧”，
/// 容易导致覆盖层错误地用失败帧替换当前冻结底图。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StitchFrameOutcome {
    /// 成功匹配位移并已更新拼接画布与基准帧（正向为扩展，负向为自动裁剪回退）
    Stitched {
        /// 沿主方向的像素位移量（正为前进扩展，负为反向回退裁剪）
        offset: i32,
        /// 匹配置信度得分（0.0 ~ 1.0）
        confidence: f32,
        /// 识别出的固定顶栏或侧边栏厚度（像素）
        sticky_header_size: u32,
    },
    /// 画面相比上一基准帧未发生有效位移，画布与基准帧保持不变
    Unchanged,
    /// 帧间重叠不足或特征差异过大，本次未更新画布且必须保留原基准帧
    Unmatched {
        /// 最佳候选位移对应的置信度得分
        confidence: f32,
    },
    /// 帧尺寸不匹配、回滚裁剪已关闭或会话当前不接受新帧，未修改任何拼接状态
    Ignored,
}

impl StitchFrameOutcome {
    /// 本帧是否实际推进或裁剪了长图画布并更新了基准帧。
    pub const fn is_stitched(self) -> bool {
        matches!(self, Self::Stitched { .. })
    }

    /// 画面是否仍处于可对齐健康状态（拼接成功或静止未变均视为绿框对齐，失配才转黄预警）。
    pub const fn is_aligned(self) -> bool {
        matches!(self, Self::Stitched { .. } | Self::Unchanged)
    }
}

/// 当前屏幕捕获视窗在整张超长位图上的映射区间。
/// 用于侧边实时缩略图悬浮面板上绘制绿色视口方框。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportWindow {
    /// 视窗起始相对坐标（主方向上，以像素计）
    pub start: f32,
    /// 视窗跨度尺寸（以像素计）
    pub size: f32,
    /// 长图当前总跨度尺寸（以像素计）
    pub total: f32,
}

impl ViewportWindow {
    /// 计算归一化起始比例 (0.0 ~ 1.0)
    pub fn normalized_start(&self) -> f32 {
        if self.total <= 0.0 {
            0.0
        } else {
            (self.start / self.total).clamp(0.0, 1.0)
        }
    }

    /// 计算归一化视口长度比例 (0.0 ~ 1.0)
    pub fn normalized_size(&self) -> f32 {
        if self.total <= 0.0 {
            1.0
        } else {
            (self.size / self.total).clamp(0.0, 1.0)
        }
    }
}

/// 侧边实时预览缩略图与局部视口映射描述符。
///
/// 之所以不再仅返回裸位图，是因为当长图达到极端长宽比（如 `800×40000`）时，
/// 整图等比塞进固定容器会让交叉轴塌缩成 `1px` 细线；本结构允许在极端场景下
/// 自动切换为“保持交叉轴最小清晰厚度的尾部最近拼接切片窗”，同时向上游暴露
/// 全局位置比例 `[slice_start_ratio, slice_end_ratio]` 用于重映射视口框及进度轨道。
#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailOverview {
    /// 已降采样生成的预览位图
    pub image: RgbaImage,
    /// 本次展示切片在整张画布主轴上的归一化起始比例 (0.0..=1.0)
    pub slice_start_ratio: f32,
    /// 本次展示切片在整张画布主轴上的归一化结束比例 (0.0..=1.0)
    pub slice_end_ratio: f32,
    /// 是否因极端长宽比启用了尾部高清窗口裁剪（`slice_start_ratio > 0.0`）
    pub is_cropped_window: bool,
}
