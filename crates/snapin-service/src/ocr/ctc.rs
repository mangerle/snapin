//! CTC (Connectionist Temporal Classification) 贪婪解码实现。

/// 对文本识别模型输出的一维扁平概率分布矩阵执行 CTC 贪婪解码。
///
/// `logits` 为 `[seq_len, num_classes]` 的扁平数组。索引 0 为 blank，
/// 索引 `1..=dict.len()` 映射字典字符，索引入 `dict.len() + 1` 映射空格
/// （PP-OCR 训练时会在字典末尾追加空格字符）。
pub fn ctc_greedy_decode(
    logits: &[f32],
    seq_len: usize,
    num_classes: usize,
    dict: &[String],
) -> (String, f32) {
    let Some(required_len) = seq_len.checked_mul(num_classes) else {
        return (String::new(), 0.0);
    };
    if seq_len == 0 || num_classes == 0 || logits.len() < required_len {
        return (String::new(), 0.0);
    }

    let mut decoded_text = String::with_capacity(seq_len);
    let mut total_score = 0.0f32;
    let mut count = 0usize;
    let mut last_index = usize::MAX;

    for timestep in 0..seq_len {
        let offset = timestep * num_classes;
        let row = &logits[offset..offset + num_classes];

        let mut max_index = 0usize;
        let mut max_value = f32::NEG_INFINITY;
        for (index, &value) in row.iter().enumerate() {
            if value > max_value {
                max_value = value;
                max_index = index;
            }
        }

        if max_index != 0 && max_index != last_index {
            if let Some(character) = map_class_to_character(max_index, dict) {
                decoded_text.push_str(character);
                if max_value.is_finite() {
                    total_score += max_value.clamp(0.0, 1.0);
                    count += 1;
                }
            }
        }

        last_index = max_index;
    }

    let average_confidence = if count > 0 {
        total_score / count as f32
    } else {
        0.0
    };

    (decoded_text, average_confidence)
}

/// 将模型输出类别索引映射为字典字符。
fn map_class_to_character(index: usize, dict: &[String]) -> Option<&str> {
    if index == 0 {
        return None;
    }

    if let Some(character) = dict.get(index - 1) {
        return Some(character.as_str());
    }

    if index == dict.len() + 1 {
        return Some(" ");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_hot_logits(sequence: &[usize], num_classes: usize) -> Vec<f32> {
        let mut logits = Vec::with_capacity(sequence.len() * num_classes);
        for &target in sequence {
            let mut row = vec![0.0f32; num_classes];
            row[target] = 0.95;
            logits.extend_from_slice(&row);
        }
        logits
    }

    #[test]
    fn test_ctc_decode_basic() {
        let dict = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let num_classes = 4;
        let sequence = [1, 1, 0, 2, 2, 3];
        let logits = one_hot_logits(&sequence, num_classes);

        let (text, confidence) = ctc_greedy_decode(&logits, sequence.len(), num_classes, &dict);
        assert_eq!(text, "ABC");
        assert!(confidence > 0.9);
    }

    #[test]
    fn test_ctc_decode_space_class() {
        let dict = vec!["A".to_string(), "B".to_string()];
        let num_classes = 4;
        let sequence = [1, 3, 2];
        let logits = one_hot_logits(&sequence, num_classes);

        let (text, _) = ctc_greedy_decode(&logits, sequence.len(), num_classes, &dict);
        assert_eq!(text, "A B");
    }

    #[test]
    fn test_ctc_decode_invalid_input_is_safe() {
        let dict = vec!["A".to_string()];
        assert_eq!(ctc_greedy_decode(&[], 0, 0, &dict), (String::new(), 0.0));
        assert_eq!(
            ctc_greedy_decode(&[0.9], 10, 10, &dict),
            (String::new(), 0.0)
        );
    }
}
