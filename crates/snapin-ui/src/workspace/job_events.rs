//! 贴图后台媒体任务结果回填。

use super::WorkspaceRuntime;
use crate::model::PasterId;
use crate::paster::window::state::PasterWindowState;
use crate::ports::{JobId, MediaJobOutcome, PasterJobKind};
use snapin_annotation::shapes::TranslationShapeItem;
use std::path::PathBuf;

/// 处理贴图后台任务结果：校验在途任务、按类别回填并请求重绘。
///
/// 任务类别与统一结果一起回传，只有类别和任务 ID 同时匹配当前在途任务才会
/// 应用结果；过期回调、错配结果与已关闭窗口都在这里被丢弃，不会污染窗口状态。
pub(super) fn handle_paster_job_finished(
    paster_id: PasterId,
    job_id: JobId,
    kind: PasterJobKind,
    outcome: MediaJobOutcome,
    runtime: &mut WorkspaceRuntime,
) {
    let Some(inst) = runtime
        .instances
        .values_mut()
        .find(|inst| inst.item_id == paster_id)
    else {
        tracing::debug!("贴图后台任务结果到达时目标贴图已关闭，结果已丢弃: {paster_id}");
        return;
    };

    if !inst.state.finish_job(kind, job_id) {
        tracing::debug!("丢弃过期或不匹配的贴图任务结果，任务类型: {kind:?}，任务序号: {job_id:?}");
        return;
    }
    apply_outcome(&mut inst.state, kind, outcome);
    inst.window.request_redraw();
}

/// 按任务类别分派统一结果；类别与结果不匹配属于编程缺陷，记录错误后丢弃。
fn apply_outcome(state: &mut PasterWindowState, kind: PasterJobKind, outcome: MediaJobOutcome) {
    match (kind, outcome) {
        (PasterJobKind::Ocr, MediaJobOutcome::Ocr { text, .. }) => apply_ocr_text(state, text),
        (PasterJobKind::Translate, MediaJobOutcome::Translated { items }) => {
            apply_translation_items(state, items);
        }
        (PasterJobKind::Save, MediaJobOutcome::Saved { path }) => apply_saved_path(state, path),
        (PasterJobKind::Save, MediaJobOutcome::SaveCancelled) => {
            state.set_status_message(snapin_foundation::i18n::tr("已取消保存"));
        }
        (kind, MediaJobOutcome::Failed { message }) => apply_failure(state, kind, message),
        (kind, outcome) => {
            tracing::error!(
                "贴图后台任务结果与任务类别不匹配，任务类型: {kind:?}，结果: {outcome:?}"
            );
        }
    }
}

/// 按任务类别生成失败提示；日志由这里统一记录一次，避免底层重复输出。
fn apply_failure(state: &mut PasterWindowState, kind: PasterJobKind, message: String) {
    match kind {
        PasterJobKind::Ocr => {
            tracing::warn!("贴图整图文字识别失败: {message}");
            state.set_status_message(snapin_foundation::trf!(
                "文字识别失败: {}",
                "Text recognition failed: {}",
                message
            ));
        }
        PasterJobKind::Translate => {
            tracing::warn!("贴图整图翻译失败: {message}");
            state.set_status_message(snapin_foundation::trf!(
                "翻译失败: {}",
                "Translation failed: {}",
                message
            ));
        }
        PasterJobKind::Save => {
            tracing::warn!("贴图保存失败: {message}");
            state.set_status_message(snapin_foundation::trf!(
                "保存失败: {}",
                "Save failed: {}",
                message
            ));
        }
    }
}

/// 处理整图 OCR 结果：成功写入剪贴板并提示字符数，空结果给出提示。
fn apply_ocr_text(state: &mut PasterWindowState, text: String) {
    let text = text.trim();
    if text.is_empty() {
        state.set_status_message(snapin_foundation::i18n::tr("未识别到文字"));
        return;
    }
    let chars = text.chars().count();
    let success_message = snapin_foundation::trf!(
        "已复制识别文字（{} 字符）",
        "Recognized text copied ({} characters)",
        chars
    );
    state.copy_text_to_clipboard(text.to_string(), success_message, "复制识别文字");
}

/// 处理整图翻译结果：作为一个复合标注注入历史，空结果视为未识别到文字。
fn apply_translation_items(state: &mut PasterWindowState, items: Vec<TranslationShapeItem>) {
    if items.is_empty() {
        state.set_status_message(snapin_foundation::i18n::tr("未识别到文字"));
        return;
    }
    let count = state.apply_translation_items(items);
    state.set_status_message(snapin_foundation::trf!(
        "已翻译 {} 段文字",
        "Translated {} text blocks",
        count
    ));
    tracing::info!("贴图整图翻译完成，共注入 {count} 段译文");
}

/// 回填保存成功后的文件路径与状态提示。
///
/// 保存通知与“保存后打开目录”由应用层媒体执行器完成，UI 只更新窗口状态，
/// 避免表现层重复解释通知配置。
fn apply_saved_path(state: &mut PasterWindowState, path: PathBuf) {
    state.file_path = Some(path.clone());
    state.set_status_message(snapin_foundation::trf!(
        "图像已保存至: {}",
        "Image saved to: {}",
        path.display()
    ));
}
