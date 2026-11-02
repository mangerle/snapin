//! 贴图文档：图像本体、标注历史与撤销所需底图。
//!
//! 文档只描述“这张贴图有什么内容”，窗口外观、任务槽与渲染缓存不应反向混入。
//! 目前保留字段级可见性以便搬迁渲染代码，后续所有修改都应通过状态层方法维护
//! `base_pixmap` 与 `annotations` 的同步不变量。

use crate::model::PasterItem;
use snapin_annotation::AnnotationHistory;
use tiny_skia::Pixmap;

/// 贴图内容文档。
pub(crate) struct PasterDocument {
    /// 当前贴图图像与几何变换
    pub(crate) item: PasterItem,
    /// 撤销/重做所需的原始底图；无标注时不复制
    pub(crate) base_pixmap: Option<Pixmap>,
    /// 已提交标注历史（图像像素坐标）
    pub(crate) annotations: AnnotationHistory,
}

impl PasterDocument {
    /// 用贴图项创建空文档。
    pub(crate) fn new(item: PasterItem) -> Self {
        Self {
            item,
            base_pixmap: None,
            annotations: AnnotationHistory::default(),
        }
    }
}
