//! 贴图后台媒体任务结果回填。

use super::WorkspaceRuntime;
use crate::model::PasterId;
use crate::paster::window::state::PasterWindowState;
use crate::ports::JobId;
use snapin_annotation::shapes::TranslationShapeItem;
use std::path::PathBuf;

/// 处理贴图保存结果：回填文件路径与状态提示。
///
/// 保存通知与“保存后打开目录”由应用层媒体执行器完成，UI 只负责更新窗口状态，
/// 避免表现层重复解释通知配置。
pub(super) fn handle_save_finished(
    paster_id: PasterId,
    job_id: JobId,
    result: Result<Option<PathBuf>, String>,
    runtime: &mut WorkspaceRuntime,
) {
    let Some(inst) = runtime
        .instances
        .values_mut()
        .find(|inst| inst.item_id == paster_id)
    else {
        tracing::warn!("保存结果到达时目标贴图已关闭，结果已丢弃: {paster_id}");
        return;
    };

    if !inst.state.finish_job(job_id) {
        tracing::debug!("丢弃过期的贴图保存结果: {job_id:?}");
        return;
    }
    match result {
        Ok(Some(path)) => {
            inst.state.file_path = Some(path.clone());
            inst.state.set_status_message(snapin_foundation::trf!(
                "图像已保存至: {}",
                "Image saved to: {}",
                path.display()
            ));
        }
        Ok(None) => inst
            .state
            .set_status_message(snapin_foundation::i18n::tr("已取消保存")),
        Err(message) => {
            tracing::warn!("贴图保存失败: {message}");
            inst.state.set_status_message(snapin_foundation::trf!(
                "保存失败: {}",
                "Save failed: {}",
                message
            ));
        }
    }

    inst.window.request_redraw();
}

/// 将后台 OCR 结果回填到发起任务的贴图窗口。
pub(super) fn handle_ocr_finished(
    paster_id: PasterId,
    job_id: JobId,
    result: Result<String, String>,
    runtime: &mut WorkspaceRuntime,
) {
    let Some(inst) = runtime
        .instances
        .values_mut()
        .find(|inst| inst.item_id == paster_id)
    else {
        tracing::debug!("文字识别结果到达时目标贴图已关闭，结果已丢弃: {paster_id}");
        return;
    };

    if !inst.state.finish_job(job_id) {
        tracing::debug!("丢弃过期的贴图识别结果: {job_id:?}");
        return;
    }
    apply_ocr_result(&mut inst.state, result);
    inst.window.request_redraw();
}

/// 将后台翻译结果注入发起任务的贴图窗口标注历史并重建展示图像。
pub(super) fn handle_translate_finished(
    paster_id: PasterId,
    job_id: JobId,
    result: Result<Vec<TranslationShapeItem>, String>,
    runtime: &mut WorkspaceRuntime,
) {
    let Some(inst) = runtime
        .instances
        .values_mut()
        .find(|inst| inst.item_id == paster_id)
    else {
        tracing::debug!("翻译结果到达时目标贴图已关闭，结果已丢弃: {paster_id}");
        return;
    };

    if !inst.state.finish_job(job_id) {
        tracing::debug!("丢弃过期的贴图翻译结果: {job_id:?}");
        return;
    }
    apply_translate_result(&mut inst.state, result);
    inst.window.request_redraw();
}

/// 处理整图 OCR 结果：成功写入剪贴板并提示字符数，失败与空结果给出中文提示。
fn apply_ocr_result(state: &mut PasterWindowState, result: Result<String, String>) {
    match result {
        Ok(text) => {
            let text = text.trim();
            if text.is_empty() {
                state.set_status_message(snapin_foundation::i18n::tr("未识别到文字"));
                return;
            }
            match state.services.desktop.write_text_clipboard(text) {
                Ok(()) => {
                    let chars = text.chars().count();
                    state.set_status_message(snapin_foundation::trf!(
                        "已复制识别文字（{} 字符）",
                        "Recognized text copied ({} characters)",
                        chars
                    ));
                    tracing::info!("贴图整图文字识别完成，已复制 {chars} 个字符到剪贴板");
                }
                Err(error) => {
                    tracing::warn!("复制贴图识别文字到系统剪贴板失败: {error}");
                    state.set_status_message(snapin_foundation::trf!(
                        "复制识别文字失败: {}",
                        "Failed to copy recognized text: {}",
                        error
                    ));
                }
            }
        }
        Err(message) => {
            tracing::warn!("贴图整图文字识别失败: {message}");
            state.set_status_message(snapin_foundation::trf!(
                "文字识别失败: {}",
                "Text recognition failed: {}",
                message
            ));
        }
    }
}

/// 处理整图翻译结果：作为一个复合标注注入历史，空结果视为未识别到文字。
fn apply_translate_result(
    state: &mut PasterWindowState,
    result: Result<Vec<TranslationShapeItem>, String>,
) {
    match result {
        Ok(items) => {
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
        Err(message) => {
            tracing::warn!("贴图整图翻译失败: {message}");
            state.set_status_message(snapin_foundation::trf!(
                "翻译失败: {}",
                "Translation failed: {}",
                message
            ));
        }
    }
}
