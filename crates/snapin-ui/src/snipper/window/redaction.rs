//! 智能打码预览状态与提交逻辑。
//!
//! OCR 完成后只生成待确认区域，不立即修改标注历史。用户可在马赛克与局部模糊
//! 之间切换，按 Enter 一次性提交，或按 Esc 放弃。这样把高风险隐私操作明确拆成
//! “识别预览”和“用户确认”两个阶段。

use super::state::SnipperState;
use crate::ports::OcrTextItem;
use crate::snipper::selection::Rect;
use snapin_core::annotation::shapes::{BlurAction, MosaicAction};
use snapin_core::annotation::AnnotationShape;

/// 智能打码使用的遮挡样式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RedactionStyle {
    /// 像素网格马赛克
    #[default]
    Mosaic,
    /// 局部模糊
    Blur,
}

impl RedactionStyle {
    /// 在两种可用样式之间切换。
    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::Mosaic => Self::Blur,
            Self::Blur => Self::Mosaic,
        }
    }

    /// 状态栏展示名称。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Mosaic => "马赛克",
            Self::Blur => "模糊",
        }
    }
}

/// 待用户确认的智能打码区域。
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionPreview {
    /// 合并后的文本包围盒，全屏坐标系
    pub regions: Vec<Rect>,
    /// 当前选择遮挡样式
    pub style: RedactionStyle,
}

impl RedactionPreview {
    /// 从 OCR 文本行构建预览，忽略零尺寸和无效包围盒。
    pub(crate) fn from_ocr_lines(lines: &[OcrTextItem]) -> Self {
        let mut regions = Vec::with_capacity(lines.len());
        for line in lines {
            let (x, y, width, height) = line.bounds;
            if width > 1.0 && height > 1.0 {
                regions.push(Rect {
                    x,
                    y,
                    width,
                    height,
                });
            }
        }
        Self {
            regions,
            style: RedactionStyle::Mosaic,
        }
    }
}

/// 把当前预览中的全部区域提交到标注历史。
pub(crate) fn commit_redaction(state: &mut SnipperState) -> usize {
    let Some(preview) = state.redaction_preview.take() else {
        return 0;
    };
    let count = preview.regions.len();
    for rect in preview.regions {
        let shape = match preview.style {
            RedactionStyle::Mosaic => AnnotationShape::Mosaic(MosaicAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                cell_size: 12,
            }),
            RedactionStyle::Blur => AnnotationShape::Blur(BlurAction {
                rect: (rect.x, rect.y, rect.width, rect.height),
                radius: 12,
            }),
        };
        state.annotations.push(shape);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: f32, y: f32, width: f32, height: f32) -> OcrTextItem {
        OcrTextItem {
            text: "敏感文字".to_string(),
            bounds: (x, y, width, height),
        }
    }

    /// 零尺寸 OCR 框不得进入预览，避免生成不可见历史项。
    #[test]
    fn test_preview_filters_invalid_boxes() {
        let preview = RedactionPreview::from_ocr_lines(&[
            line(10.0, 20.0, 80.0, 20.0),
            line(0.0, 0.0, 0.0, 10.0),
        ]);
        assert_eq!(preview.regions.len(), 1);
    }

    /// 切换样式只能得到另一个合法枚举值。
    #[test]
    fn test_style_toggle() {
        assert_eq!(RedactionStyle::Mosaic.toggle(), RedactionStyle::Blur);
        assert_eq!(RedactionStyle::Blur.toggle(), RedactionStyle::Mosaic);
    }
}
