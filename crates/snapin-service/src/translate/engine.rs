//! 翻译服务抽象特型。

use crate::error::ServiceError;
use async_trait::async_trait;

/// 统一翻译服务接口特型
#[async_trait]
pub trait Translator: Send + Sync {
    /// 批量翻译文本列表，返回与入参保持严格下标顺序对应的译文列表
    async fn translate_batch(
        &self,
        texts: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, ServiceError>;

    /// 单句翻译辅助方法
    async fn translate(&self, text: &str, from: &str, to: &str) -> Result<String, ServiceError> {
        let results = self.translate_batch(&[text.to_string()], from, to).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| ServiceError::ApiError {
                provider: "unknown".to_string(),
                code: "EMPTY_RESULT".to_string(),
                message: "未返回有效的翻译结果".to_string(),
            })
    }
}
