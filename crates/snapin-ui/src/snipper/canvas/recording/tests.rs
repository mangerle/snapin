use crate::editor::toolbar::Toolbar;
use crate::geometry::Rect;
use crate::ports::NoopScreenRecorder;
use crate::snipper::canvas::recording::render::format_elapsed;
use crate::snipper::canvas::recording::{
    calculate_recording_layout, render_recording_toolbar, RecordingLayoutRequest,
    RecordingRenderContext,
};
use crate::snipper::window::state::RecordingSession;
use std::sync::Arc;
use tiny_skia::{Color, Pixmap};

#[test]
fn test_format_elapsed_switches_to_hours() {
    assert_eq!(format_elapsed(5), "00:05");
    assert_eq!(format_elapsed(65), "01:05");
    assert_eq!(format_elapsed(3661), "01:01:01");
}

#[test]
fn test_render_recording_toolbar_with_settings_menu() {
    let mut pixmap = Pixmap::new(900, 180).expect("测试位图应可分配");
    pixmap.fill(Color::from_rgba8(68, 112, 140, 255));
    let mut session = RecordingSession::new(Arc::new(NoopScreenRecorder), Arc::new(|_| {}));
    session.toggle_settings();
    let toolbar = Toolbar::default();
    let selection = Rect::new(40.0, 40.0, 820.0, 100.0);
    let layout = calculate_recording_layout(RecordingLayoutRequest {
        session: &session,
        selection: &selection,
        screen_width: 900.0,
        screen_height: 180.0,
        scale: 1.0,
    });
    assert!(layout.menu_rect.is_some(), "设置菜单应处于展开状态");
    render_recording_toolbar(
        &mut pixmap.as_mut(),
        RecordingRenderContext {
            session: &session,
            toolbar: &toolbar,
            selection: &selection,
            screen_size: (900, 180),
            ui_scale: 1.0,
        },
    );
    let sample_x = layout.bar_rect.x as u32 + 4;
    let sample_y = layout.bar_rect.y as u32 + 4;
    let pixel = pixmap
        .pixel(sample_x, sample_y)
        .expect("工具栏采样像素应存在");
    assert!(pixel.alpha() > 0, "工具栏底板必须实际绘制");
}

#[test]
#[ignore = "用于生成录制工具栏多状态视觉预览"]
fn test_render_recording_toolbar_gallery() {
    use crate::editor::toolbar::Tool;
    use crate::ports::RecordingEvent;
    use crate::snipper::window::state::{RecordingAction, RecordingItem};
    use std::time::Duration;

    let Ok(output_dir) = std::env::var("SNAPIN_RECORDING_PREVIEW_DIR") else {
        return;
    };
    std::fs::create_dir_all(&output_dir).expect("预览输出目录应可创建");

    let selection = Rect::new(40.0, 40.0, 820.0, 100.0);
    let mut toolbar = Toolbar::default();
    toolbar.active_tool = Some(Tool::Pen);

    let idle = RecordingSession::new(Arc::new(NoopScreenRecorder), Arc::new(|_| {}));
    render_preview(
        &idle,
        &toolbar,
        &selection,
        Color::from_rgba8(49, 88, 112, 255),
        &format!("{output_dir}/01-idle-default.png"),
    );

    let mut options = RecordingSession::new(Arc::new(NoopScreenRecorder), Arc::new(|_| {}));
    options.toggle_audio();
    options.toggle_cursor();
    options.set_format(crate::ports::RecordingFormat::WebP);
    options.cycle_fps();
    options.set_hovered(Some(RecordingItem::Action(RecordingAction::ToggleAudio)));
    render_preview(
        &options,
        &toolbar,
        &selection,
        Color::from_rgba8(58, 84, 112, 255),
        &format!("{output_dir}/02-idle-options.png"),
    );

    let mut recording = RecordingSession::new(Arc::new(NoopScreenRecorder), Arc::new(|_| {}));
    recording.toggle_audio();
    recording.toggle_cursor();
    recording.handle_event(RecordingEvent::Started);
    recording.handle_event(RecordingEvent::Progress {
        elapsed: Duration::from_secs(12),
        frames: 720,
    });
    recording.set_hovered(Some(RecordingItem::Action(RecordingAction::TogglePause)));
    render_preview(
        &recording,
        &toolbar,
        &selection,
        Color::from_rgba8(44, 96, 91, 255),
        &format!("{output_dir}/03-recording.png"),
    );

    let mut paused = RecordingSession::new(Arc::new(NoopScreenRecorder), Arc::new(|_| {}));
    paused.toggle_audio();
    paused.toggle_cursor();
    paused.handle_event(RecordingEvent::Started);
    paused.handle_event(RecordingEvent::Paused);
    paused.handle_event(RecordingEvent::Progress {
        elapsed: Duration::from_secs(24),
        frames: 1440,
    });
    paused.set_hovered(Some(RecordingItem::Action(RecordingAction::TogglePause)));
    render_preview(
        &paused,
        &toolbar,
        &selection,
        Color::from_rgba8(91, 78, 48, 255),
        &format!("{output_dir}/04-paused.png"),
    );

    let mut settings = RecordingSession::new(Arc::new(NoopScreenRecorder), Arc::new(|_| {}));
    settings.toggle_settings();
    settings.set_hovered(Some(RecordingItem::Action(RecordingAction::ToggleSettings)));
    render_preview(
        &settings,
        &toolbar,
        &selection,
        Color::from_rgba8(62, 72, 102, 255),
        &format!("{output_dir}/05-settings-menu.png"),
    );

    let mut tooltip = RecordingSession::new(Arc::new(NoopScreenRecorder), Arc::new(|_| {}));
    tooltip.set_hovered(Some(RecordingItem::Action(RecordingAction::Restart)));
    render_preview(
        &tooltip,
        &toolbar,
        &selection,
        Color::from_rgba8(72, 76, 86, 255),
        &format!("{output_dir}/06-hover-tooltip.png"),
    );
}

fn render_preview(
    session: &RecordingSession,
    toolbar: &Toolbar,
    selection: &Rect,
    background: Color,
    output_path: &str,
) {
    let mut pixmap = Pixmap::new(900, 220).expect("测试位图应可分配");
    pixmap.fill(background);
    render_recording_toolbar(
        &mut pixmap.as_mut(),
        RecordingRenderContext {
            session,
            toolbar,
            selection,
            screen_size: (900, 220),
            ui_scale: 1.0,
        },
    );
    pixmap.save_png(output_path).expect("预览图应可保存");
}
