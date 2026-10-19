//! OCR 数据契约与文本块元数据定义。

use serde::{Deserialize, Serialize};

/// 单个检测与识别出的文本行/块元数据
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrTextBlock {
    /// 识别出的文本内容
    pub text: String,
    /// 文本块局部的轴对齐外接矩形 [x, y, width, height]
    pub bbox: (f32, f32, f32, f32),
    /// 文本四边形 4 个顶点坐标 (顺时针，用于更精准的原图涂抹与旋转校正)
    pub polygon: [(f32, f32); 4],
    /// 识别置信度 (0.0 ~ 1.0)
    pub confidence: f32,
    /// 文本框边缘估计背景主导色 RGBA (用于原位擦除)
    pub bg_color: (u8, u8, u8, u8),
    /// 文本字体估计前景色 RGBA
    pub text_color: (u8, u8, u8, u8),
}

impl OcrTextBlock {
    /// 构造新的文本块实例
    pub fn new(
        text: String,
        bbox: (f32, f32, f32, f32),
        polygon: [(f32, f32); 4],
        confidence: f32,
    ) -> Self {
        Self {
            text,
            bbox,
            polygon,
            confidence,
            bg_color: (255, 255, 255, 255), // 默认白底
            text_color: (24, 24, 27, 255),  // 默认黑字
        }
    }
}

/// 选区完整的 OCR 识别结果集合
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OcrResult {
    /// 所有检测识别到的文本块列表
    pub blocks: Vec<OcrTextBlock>,
    /// 按照自然阅读顺序规整后的全文段落文本 (用换行符连接)
    pub full_text: String,
}

impl OcrResult {
    /// 根据文本块列表构建聚合结果
    pub fn from_blocks(blocks: Vec<OcrTextBlock>) -> Self {
        let full_text = blocks
            .iter()
            .map(|b| b.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        Self { blocks, full_text }
    }
}
