//! 长截图拼接领域模型、选项与几何视口类型定义。

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
