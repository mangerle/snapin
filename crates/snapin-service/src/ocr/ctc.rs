//! CTC (Connectionist Temporal Classification) 贪婪解码实现。

/// 对文本识别模型输出的一维扁平概率分布矩阵执行 CTC 贪婪解码
///
/// `logits`: [seq_len, num_classes] 的扁平浮点数组
/// `dict`: 字符字典数组 (索引 0 映射为空白标记 blank)
pub fn ctc_greedy_decode(
    logits: &[f32],
    seq_len: usize,
    num_classes: usize,
    dict: &[String],
) -> (String, f32) {
    if seq_len == 0 || num_classes == 0 || logits.is_empty() {
        return (String::new(), 0.0);
    }

    let mut decoded_text = String::with_capacity(seq_len);
    let mut total_score = 0.0f32;
    let mut count = 0usize;
    let mut last_idx = 0usize;

    for t in 0..seq_len {
        let offset = t * num_classes;
        let step_slice = &logits[offset..offset + num_classes];

        // 寻找当前时间步长概率最大的类别
        let mut max_idx = 0;
        let mut max_val = f32::NEG_INFINITY;
        for (i, &val) in step_slice.iter().enumerate() {
            if val > max_val {
                max_val = val;
                max_idx = i;
            }
        }

        // CTC 解码核心规则:
        // 1. 跳过 blank 标记 (索引 0)
        // 2. 合并连续相同字符 (例如 [A, A] -> A，但 [A, blank, A] -> AA)
        if max_idx != 0 && max_idx != last_idx {
            // 字典索引对齐: 0 为空白标记，字典首个字符通常对应索引 1 (或视导出规范对齐)
            let char_idx = max_idx.saturating_sub(1);
            if let Some(ch) = dict.get(char_idx) {
                decoded_text.push_str(ch);
                total_score += max_val;
                count += 1;
            }
        }

        last_idx = max_idx;
    }

    let avg_confidence = if count > 0 {
        // 若 logits 尚未进行 softmax，可进行简易归一化或截断
        (total_score / count as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    (decoded_text, avg_confidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctc_decode_basic() {
        let dict = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let num_classes = 4; // 0: blank, 1: A, 2: B, 3: C

        // 构造时间步: [A, A, blank, B, B, C]
        let seq = vec![1, 1, 0, 2, 2, 3];
        let mut logits = Vec::new();
        for &target in &seq {
            let mut row = vec![0.0f32; num_classes];
            row[target] = 0.95;
            logits.extend_from_slice(&row);
        }

        let (text, conf) = ctc_greedy_decode(&logits, seq.len(), num_classes, &dict);
        assert_eq!(text, "ABC");
        assert!(conf > 0.9);
    }
}
