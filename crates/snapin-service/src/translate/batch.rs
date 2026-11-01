//! 翻译批量规划与多段结果合并。
//!
//! 免费谷歌接口把文本放在 URL 查询串中，百度/有道/阿里则把整批文本合并为一个
//! `q` 参数；两者都存在明确的长度上限。此前调用方把全部 OCR 文本一次塞入请求，
//! 长文档会因 URL 过长或 `q` 超限整批失败，原文含换行时还会破坏“返回行数等于
//! 输入行数”的推断。本模块集中处理分片、换行归一化与空文本占位，避免每个
//! provider 各写一套边界逻辑。

/// 单批文本规划参数。
#[derive(Debug, Clone, Copy)]
pub(crate) struct BatchLimits {
    /// 单批最大文本条数
    pub(crate) max_items: usize,
    /// 单批文本总字节上限（换行分隔符计入）
    pub(crate) max_total_bytes: usize,
    /// 单条文本字节上限，超长文本会独占一批
    pub(crate) max_text_bytes: usize,
}

impl BatchLimits {
    /// 构造一组至少包含 1 条、1 字节的合法限制，避免零值导致死循环。
    pub(crate) fn new(max_items: usize, max_total_bytes: usize, max_text_bytes: usize) -> Self {
        Self {
            max_items: max_items.max(1),
            max_total_bytes: max_total_bytes.max(1),
            max_text_bytes: max_text_bytes.max(1),
        }
    }
}

/// 按条数与字节预算把文本切成连续批次，保持原始顺序。
pub(crate) fn plan_batches(texts: &[String], limits: BatchLimits) -> Vec<&[String]> {
    let mut batches = Vec::new();
    let mut start = 0usize;

    while start < texts.len() {
        let mut end = start;
        let mut total_bytes = 0usize;

        while end < texts.len() && end - start < limits.max_items {
            let text_bytes = texts[end].len();
            let separator = usize::from(end > start);
            let next_total = total_bytes
                .saturating_add(separator)
                .saturating_add(text_bytes);

            if end > start && next_total > limits.max_total_bytes {
                break;
            }
            total_bytes = next_total;
            end += 1;

            // 超长文本独占一批，避免它把后续短文本一起拖成超限请求。
            if text_bytes > limits.max_text_bytes {
                break;
            }
        }

        if end == start {
            end = start + 1;
        }
        batches.push(&texts[start..end]);
        start = end;
    }

    batches
}

/// 将一批文本合并为单个 `q` 参数所需的文本与原始下标。
///
/// 换行替换为空格，空文本不参与请求，从而保证返回条数与参与请求的文本条数一致；
/// 调用方再用 [`merge_joined_results`] 把空文本位置补回空字符串。
pub(crate) fn join_batch(batch: &[String]) -> (String, Vec<usize>) {
    let mut parts = Vec::with_capacity(batch.len());
    let mut indices = Vec::with_capacity(batch.len());
    for (index, text) in batch.iter().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        parts.push(text.replace(['\r', '\n'], " "));
        indices.push(index);
    }
    (parts.join("\n"), indices)
}

/// 把合并请求返回的译文按原始位置回填；
/// 空文本位置保持空字符串，返回条数不符时返回明确错误。
pub(crate) fn merge_joined_results(
    total_items: usize,
    indices: &[usize],
    translated: Vec<String>,
) -> Result<Vec<String>, crate::ServiceError> {
    if translated.len() != indices.len() {
        return Err(crate::ServiceError::ApiError {
            provider: "batch".to_string(),
            code: "RESULT_COUNT_MISMATCH".to_string(),
            message: format!(
                "合并翻译返回 {} 条译文，与参与请求的 {} 条文本不一致",
                translated.len(),
                indices.len()
            ),
        });
    }

    let mut results = vec![String::new(); total_items];
    for (index, text) in indices.iter().zip(translated) {
        if let Some(slot) = results.get_mut(*index) {
            *slot = text;
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 分片必须保持顺序、覆盖全部文本，且每批不超预算。
    #[test]
    fn test_plan_batches_preserves_order_and_budget() {
        let texts = vec![
            "a".to_string(),
            "bb".to_string(),
            "ccc".to_string(),
            "dddd".to_string(),
        ];
        let batches = plan_batches(&texts, BatchLimits::new(2, 8, 10));
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0], &texts[0..2]);
        assert_eq!(batches[1], &texts[2..4]);
    }

    /// 超长文本必须独占一批，不能与后续文本合并。
    #[test]
    fn test_overlong_text_gets_its_own_batch() {
        let texts = vec!["x".repeat(20), "short".to_string()];
        let batches = plan_batches(&texts, BatchLimits::new(10, 100, 10));
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[1].len(), 1);
    }

    /// 空文本不进入请求，但在最终结果中保留空占位。
    #[test]
    fn test_join_and_merge_keeps_empty_slots() {
        let batch = vec!["hello".to_string(), "  ".to_string(), "world".to_string()];
        let (joined, indices) = join_batch(&batch);
        assert_eq!(joined, "hello\nworld");
        assert_eq!(indices, vec![0, 2]);

        let merged =
            merge_joined_results(batch.len(), &indices, vec!["你好".into(), "世界".into()])
                .expect("条数匹配时应成功");
        assert_eq!(merged, vec!["你好", "", "世界"]);
    }

    /// 原文换行必须归一化为空格，避免破坏按行推断的返回条数。
    #[test]
    fn test_join_batch_normalizes_newlines() {
        let batch = vec!["a\nb".to_string(), "c".to_string()];
        let (joined, _) = join_batch(&batch);
        assert_eq!(joined, "a b\nc");
    }
}
