use super::*;

/// 只阻止检查与下载中的重复点击，已发现新版本或错误状态仍允许重试。
#[test]
fn test_update_is_busy_only_blocks_inflight_states() {
    assert!(update_is_busy(UpdateUiStatus::Checking as i32));
    assert!(update_is_busy(UpdateUiStatus::Downloading as i32));
    assert!(!update_is_busy(UpdateUiStatus::Idle as i32));
    assert!(!update_is_busy(UpdateUiStatus::Available as i32));
    assert!(!update_is_busy(UpdateUiStatus::Error as i32));
}
