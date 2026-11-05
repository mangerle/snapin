//! 贴图窗口媒体任务提交与结果回传门面。
//!
//! # 设计原理
//!
//! 贴图窗口运行在 winit 事件循环线程上，任何 ONNX 推理、HTTP 翻译或图像编码都会
//! 直接冻结界面。真正的后台执行由应用层 [`MediaJobRunner`] 实现，本模块只负责：
//! 1. 把窗口动作转换为 [`MediaJob`]；
//! 2. 携带 `paster_id` 提交任务，保证结果精确路由回发起窗口；
//! 3. 把端口结果转换为工作区用户事件。
//!
//! UI 不再持有工作线程、服务缓存或配置快照；工作区配置热更新由应用层执行器统一失效，
//! 避免截图侧与贴图侧各自维护一套 OCR/翻译缓存。

use crate::model::PasterId;
use crate::ports::{JobCancel, JobId, MediaJob, MediaJobRunner, PasterJobKind, PortError};
use crate::workspace::PasterWorkspaceProxy;
use image::RgbaImage;
use snapin_config::config::CoverMode;
use std::sync::Arc;
use tiny_skia::Pixmap;

/// 贴图保存任务的交互选项。
#[derive(Debug, Clone, Copy)]
pub(crate) struct PasterSaveOptions {
    /// 是否打开系统另存为对话框；为否时使用快捷保存路径。
    pub(crate) as_dialog: bool,
    /// 保存完成后是否打开文件所在目录。
    pub(crate) open_folder_after: bool,
}

/// 将 tiny-skia 位图转换为 OCR 引擎可直接消费的 RGBA 图像。
///
/// `Pixmap::data()` 为预乘 RGBA 字节序，与引擎入参的通道布局一致；
/// 这里仍显式校验尺寸与字节长度，避免上游异常导致越界读或尺寸错位。
pub(crate) fn pixmap_to_rgba_image(pixmap: &Pixmap) -> Result<RgbaImage, String> {
    rgba_from_premultiplied(pixmap.width(), pixmap.height(), pixmap.data())
}

/// 从预乘 RGBA 裸字节构造图像，并校验宽高乘积与缓冲区长度。
fn rgba_from_premultiplied(width: u32, height: u32, data: &[u8]) -> Result<RgbaImage, String> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| format!("图像尺寸溢出，宽度: {width}，高度: {height}"))?;
    if data.len() != expected {
        return Err(format!(
            "图像像素长度不匹配，期望 {expected} 字节，实际 {} 字节",
            data.len()
        ));
    }
    RgbaImage::from_raw(width, height, data.to_vec())
        .ok_or_else(|| format!("图像尺寸不合法，宽度: {width}，高度: {height}"))
}

/// 贴图工作区共享的媒体任务运行时。
///
/// 单个实例被工作区内所有贴图窗口共享，但每个任务都携带 `paster_id`，
/// 因此多个窗口可以并发提交任务而不会把结果投递到错误的窗口。
pub(crate) struct PasterJobRuntime {
    runner: Arc<dyn MediaJobRunner>,
    proxy: PasterWorkspaceProxy,
}

impl PasterJobRuntime {
    /// 使用宿主媒体执行器与工作区代理构建运行时
    pub(crate) fn new(runner: Arc<dyn MediaJobRunner>, proxy: PasterWorkspaceProxy) -> Self {
        Self { runner, proxy }
    }

    /// 提交一项贴图后台任务，并把任务类别与统一结果一起回传工作区。
    ///
    /// # Errors
    ///
    /// 后台执行器停止、队列已满或任务参数非法时返回 [`PortError`]。
    fn submit(
        &self,
        paster_id: PasterId,
        kind: PasterJobKind,
        job: MediaJob,
    ) -> Result<(JobId, JobCancel), PortError> {
        let proxy = self.proxy.clone();
        self.runner.submit(
            job,
            Box::new(move |job_id, outcome| {
                if let Err(error) = proxy.paster_job_finished(paster_id, job_id, kind, outcome) {
                    tracing::warn!("贴图工作区已关闭，后台任务结果未能回传: {error}");
                }
            }),
        )
    }

    /// 提交整图文字识别。
    ///
    /// # Errors
    ///
    /// 后台执行器停止、队列已满或任务参数非法时返回 [`PortError`]。
    pub(crate) fn submit_ocr(
        &self,
        paster_id: PasterId,
        image: RgbaImage,
    ) -> Result<(JobId, JobCancel), PortError> {
        self.submit(
            paster_id,
            PasterJobKind::Ocr,
            MediaJob::Ocr {
                image,
                origin: (0, 0),
            },
        )
    }

    /// 提交整图 OCR 后原位翻译。
    ///
    /// # Errors
    ///
    /// 与 [`Self::submit_ocr`] 相同。
    pub(crate) fn submit_translate(
        &self,
        paster_id: PasterId,
        image: RgbaImage,
        cover_mode: CoverMode,
    ) -> Result<(JobId, JobCancel), PortError> {
        self.submit(
            paster_id,
            PasterJobKind::Translate,
            MediaJob::Translate {
                image,
                origin: (0, 0),
                cover_mode,
            },
        )
    }

    /// 提交贴图保存任务。
    ///
    /// # Errors
    ///
    /// 与 [`Self::submit_ocr`] 相同。
    pub(crate) fn submit_save(
        &self,
        paster_id: PasterId,
        image: RgbaImage,
        options: PasterSaveOptions,
    ) -> Result<(JobId, JobCancel), PortError> {
        self.submit(
            paster_id,
            PasterJobKind::Save,
            MediaJob::SavePaster {
                image,
                as_dialog: options.as_dialog,
                open_folder_after: options.open_folder_after,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Color;

    /// 位图转换应保持宽高与字节长度一致
    #[test]
    fn 位图转换保持尺寸与像素长度() {
        let mut pixmap = Pixmap::new(4, 3).expect("创建测试位图失败");
        pixmap.fill(Color::from_rgba8(10, 20, 30, 255));

        let image = pixmap_to_rgba_image(&pixmap).expect("位图转换应成功");
        assert_eq!(image.dimensions(), (4, 3));
        assert_eq!(image.as_raw().len(), 4 * 3 * 4);
    }

    /// 缓冲区长度与宽高不匹配时必须返回错误而不是越界读取
    #[test]
    fn 像素长度不符时返回错误() {
        let error = rgba_from_premultiplied(2, 2, &[0u8; 15]).expect_err("长度不符必须失败");
        assert!(
            error.contains("长度不匹配"),
            "错误信息应说明长度不匹配，实际: {error}"
        );
    }
}
