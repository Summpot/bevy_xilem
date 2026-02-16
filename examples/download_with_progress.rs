use std::{
    fs::File,
    io::{Read, Write},
    sync::Arc,
    time::{Duration, Instant},
};

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ColorStyle, LayoutStyle, ProjectionCtx, StyleClass,
    StyleSetter, StyleSheet, StyleTransition, TextStyle, UiEventQueue, UiRoot, UiView,
    apply_label_style, apply_text_input_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::prelude::*,
    bevy_tasks::{IoTaskPool, TaskPoolBuilder},
    button, emit_ui_action, resolve_style, resolve_style_for_classes, rfd,
    run_app_with_window_options, switch, text_input,
    xilem::{
        core::fork,
        view::{
            CrossAxisAlignment, FlexExt as _, MainAxisAlignment, flex_col, flex_row, label,
            progress_bar, task,
        },
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};

const HEARTBEAT_MS: u64 = 60;
const DEFAULT_URL: &str = "https://hil-speed.hetzner.com/100MB.bin";

#[derive(Resource, Debug, Clone)]
struct DownloadState {
    url: String,
    use_system_dialog: bool,
    in_progress: bool,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    status: String,
    active_target: Option<String>,
    completed_dialog: Option<String>,
}

impl Default for DownloadState {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.to_string(),
            use_system_dialog: false,
            in_progress: false,
            downloaded_bytes: 0,
            total_bytes: None,
            status: "Idle".to_string(),
            active_target: None,
            completed_dialog: None,
        }
    }
}

#[derive(Debug, Clone)]
enum DownloadEvent {
    SetUrl(String),
    SetUseSystemDialog(bool),
    StartDownload,
    DismissDialog,
    Tick,
    ShowSystemDialog {
        title: String,
        description: String,
    },
    SystemDialogClosed,
    WorkerStarted {
        total_bytes: Option<u64>,
        target: String,
    },
    WorkerProgress {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    WorkerFinished {
        target: String,
    },
    WorkerFailed(String),
}

#[derive(Component, Debug, Clone, Copy)]
struct DownloadRootView;

fn ensure_io_task_pool() {
    IoTaskPool::get_or_init(|| {
        TaskPoolBuilder::new()
            .thread_name("bevy_xilem IO Task Pool".to_string())
            .build()
    });
}

fn url_file_name(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|segments| segments.filter(|seg| !seg.is_empty()).next_back())
                .map(ToString::to_string)
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "download.bin".to_string())
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;

    if b >= GB {
        format!("{:.2} GiB", b / GB)
    } else if b >= MB {
        format!("{:.2} MiB", b / MB)
    } else if b >= KB {
        format!("{:.2} KiB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn spawn_download_worker(entity: Entity, url: String) {
    ensure_io_task_pool();
    IoTaskPool::get()
        .spawn(async move {
            let fail = |msg: String| {
                emit_ui_action(entity, DownloadEvent::WorkerFailed(msg));
            };

            let file_name = url_file_name(&url);
            let target = std::env::current_dir()
                .map(|dir| dir.join(&file_name))
                .unwrap_or_else(|_| file_name.into());
            let target_text = target.display().to_string();

            let client = reqwest::blocking::Client::new();
            let mut response = match client.get(&url).send() {
                Ok(response) => response,
                Err(err) => {
                    fail(format!("Request failed: {err}"));
                    return;
                }
            };

            if !response.status().is_success() {
                fail(format!("HTTP {}", response.status()));
                return;
            }

            let total_bytes = response.content_length();
            emit_ui_action(
                entity,
                DownloadEvent::WorkerStarted {
                    total_bytes,
                    target: target_text.clone(),
                },
            );

            let mut file = match File::create(&target) {
                Ok(file) => file,
                Err(err) => {
                    fail(format!("Cannot create target file: {err}"));
                    return;
                }
            };

            let mut buffer = vec![0_u8; 64 * 1024];
            let mut downloaded_bytes = 0_u64;
            let mut last_emit = Instant::now();

            loop {
                let read_count = match response.read(&mut buffer) {
                    Ok(n) => n,
                    Err(err) => {
                        fail(format!("Read failed: {err}"));
                        return;
                    }
                };

                if read_count == 0 {
                    break;
                }

                if let Err(err) = file.write_all(&buffer[..read_count]) {
                    fail(format!("Write failed: {err}"));
                    return;
                }

                downloaded_bytes += u64::try_from(read_count).unwrap_or(0);

                if last_emit.elapsed() >= Duration::from_millis(HEARTBEAT_MS) {
                    emit_ui_action(
                        entity,
                        DownloadEvent::WorkerProgress {
                            downloaded_bytes,
                            total_bytes,
                        },
                    );
                    last_emit = Instant::now();
                }
            }

            emit_ui_action(
                entity,
                DownloadEvent::WorkerProgress {
                    downloaded_bytes,
                    total_bytes,
                },
            );
            emit_ui_action(
                entity,
                DownloadEvent::WorkerFinished {
                    target: target_text,
                },
            );
        })
        .detach();
}

fn spawn_system_dialog(entity: Entity, title: String, description: String) {
    ensure_io_task_pool();
    IoTaskPool::get()
        .spawn(async move {
            let _ = rfd::MessageDialog::new()
                .set_title(&title)
                .set_description(&description)
                .set_level(rfd::MessageLevel::Info)
                .set_buttons(rfd::MessageButtons::Ok)
                .show();

            emit_ui_action(entity, DownloadEvent::SystemDialogClosed);
        })
        .detach();
}

fn progress_value(state: &DownloadState) -> Option<f64> {
    state
        .total_bytes
        .filter(|total| *total > 0)
        .map(|total| state.downloaded_bytes as f64 / total as f64)
}

fn project_download_root(_: &DownloadRootView, ctx: ProjectionCtx<'_>) -> UiView {
    let root_style = resolve_style(ctx.world, ctx.entity);
    let title_style = resolve_style_for_classes(ctx.world, ["download.title"]);
    let row_style = resolve_style_for_classes(ctx.world, ["download.row"]);
    let input_style = resolve_style_for_classes(ctx.world, ["download.url-input"]);
    let status_style = resolve_style_for_classes(ctx.world, ["download.status"]);
    let button_style = resolve_style_for_classes(ctx.world, ["download.button"]);
    let dialog_style = resolve_style_for_classes(ctx.world, ["download.dialog"]);

    let state = ctx.world.resource::<DownloadState>().clone();

    let title = apply_label_style(label("Remote File Downloader"), &title_style);

    let url_row = apply_widget_style(
        flex_row((
            apply_label_style(label("URL:"), &status_style),
            apply_text_input_style(
                text_input(ctx.entity, state.url.clone(), DownloadEvent::SetUrl)
                    .placeholder(DEFAULT_URL),
                &input_style,
            )
            .flex(1.0),
        )),
        &row_style,
    );

    let button_text = if state.in_progress {
        "Downloading..."
    } else {
        "Download"
    };

    let action_row = apply_widget_style(
        flex_row((apply_widget_style(
            button(ctx.entity, DownloadEvent::StartDownload, button_text),
            &button_style,
        ),))
        .main_axis_alignment(MainAxisAlignment::Start),
        &row_style,
    );

    let dialog_mode_row = apply_widget_style(
        flex_row((
            apply_label_style(label("Completion dialog:"), &status_style),
            switch(
                ctx.entity,
                state.use_system_dialog,
                DownloadEvent::SetUseSystemDialog,
            ),
            apply_label_style(
                label(if state.use_system_dialog {
                    "System"
                } else {
                    "Modal"
                }),
                &status_style,
            ),
        ))
        .main_axis_alignment(MainAxisAlignment::Start),
        &row_style,
    );

    let progress_text = match state.total_bytes {
        Some(total) if total > 0 => format!(
            "{} / {} ({:.1}%)",
            format_bytes(state.downloaded_bytes),
            format_bytes(total),
            (state.downloaded_bytes as f64 / total as f64) * 100.0
        ),
        _ => format!("{} downloaded", format_bytes(state.downloaded_bytes)),
    };

    let target_text = state
        .active_target
        .as_deref()
        .map(|target| format!("Target: {target}"))
        .unwrap_or_else(|| "Target: (not started)".to_string());

    let mut children = vec![
        title.into_any_flex(),
        url_row.into_any_flex(),
        action_row.into_any_flex(),
        dialog_mode_row.into_any_flex(),
        progress_bar(progress_value(&state)).into_any_flex(),
        apply_label_style(label(progress_text), &status_style).into_any_flex(),
        apply_label_style(label(target_text), &status_style).into_any_flex(),
        apply_label_style(label(state.status), &status_style).into_any_flex(),
    ];

    if let Some(message) = state.completed_dialog {
        children.push(
            apply_widget_style(
                flex_col((
                    apply_label_style(label("Download finished"), &title_style),
                    apply_label_style(label(message), &status_style),
                    apply_widget_style(
                        button(ctx.entity, DownloadEvent::DismissDialog, "OK"),
                        &button_style,
                    ),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start),
                &dialog_style,
            )
            .into_any_flex(),
        );
    }

    let content = apply_widget_style(
        flex_col(children).cross_axis_alignment(CrossAxisAlignment::Start),
        &root_style,
    );

    let heartbeat_entity = ctx.entity;
    let heartbeat = task(
        |proxy, _| async move {
            let mut interval = tokio::time::interval(Duration::from_millis(HEARTBEAT_MS));
            loop {
                interval.tick().await;
                let Ok(()) = proxy.message(()) else {
                    break;
                };
            }
        },
        move |_: (), ()| {
            emit_ui_action(heartbeat_entity, DownloadEvent::Tick);
        },
    );

    let with_heartbeat = fork(content, Some(heartbeat));
    Arc::new(with_heartbeat)
}

fn setup_download_world(mut commands: Commands) {
    commands.spawn((
        UiRoot,
        DownloadRootView,
        StyleClass(vec!["download.root".to_string()]),
    ));
}

fn setup_download_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "download.root",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(16.0),
                gap: Some(8.0),
                corner_radius: Some(10.0),
                border_width: Some(1.0),
            },
            colors: ColorStyle {
                bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x1F, 0x1F, 0x24)),
                border: Some(bevy_xilem::xilem::Color::from_rgb8(0x3A, 0x3A, 0x42)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "download.title",
        StyleSetter {
            text: TextStyle { size: Some(22.0) },
            colors: ColorStyle {
                text: Some(bevy_xilem::xilem::Color::from_rgb8(0xF5, 0xF5, 0xFA)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "download.row",
        StyleSetter {
            layout: LayoutStyle {
                gap: Some(8.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "download.url-input",
        StyleSetter {
            text: TextStyle { size: Some(14.0) },
            layout: LayoutStyle {
                padding: Some(6.0),
                border_width: Some(1.0),
                corner_radius: Some(8.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x2A, 0x2A, 0x31)),
                border: Some(bevy_xilem::xilem::Color::from_rgb8(0x47, 0x47, 0x50)),
                text: Some(bevy_xilem::xilem::Color::from_rgb8(0xEF, 0xEF, 0xF4)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "download.button",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(6.0),
                corner_radius: Some(8.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x25, 0x63, 0xEB)),
                hover_bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x1D, 0x4E, 0xD8)),
                pressed_bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x1E, 0x40, 0xAF)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.12 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "download.status",
        StyleSetter {
            text: TextStyle { size: Some(13.0) },
            colors: ColorStyle {
                text: Some(bevy_xilem::xilem::Color::from_rgb8(0xD6, 0xD6, 0xDD)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "download.dialog",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                gap: Some(8.0),
                border_width: Some(1.0),
                corner_radius: Some(8.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x26, 0x26, 0x2F)),
                border: Some(bevy_xilem::xilem::Color::from_rgb8(0x54, 0x54, 0x66)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );
}

fn drain_download_events(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<DownloadEvent>();

    if events.is_empty() {
        return;
    }

    for event in events {
        match event.action {
            DownloadEvent::SetUrl(url) => {
                world.resource_mut::<DownloadState>().url = url;
            }
            DownloadEvent::SetUseSystemDialog(value) => {
                let mut state = world.resource_mut::<DownloadState>();
                state.use_system_dialog = value;
                if value {
                    state.completed_dialog = None;
                }
            }
            DownloadEvent::StartDownload => {
                let (entity, url, should_start) = {
                    let mut state = world.resource_mut::<DownloadState>();
                    if state.in_progress {
                        state.status = "A download is already in progress.".to_string();
                        (event.entity, String::new(), false)
                    } else {
                        state.in_progress = true;
                        state.downloaded_bytes = 0;
                        state.total_bytes = None;
                        state.active_target = None;
                        state.completed_dialog = None;
                        state.status = "Starting download...".to_string();
                        (event.entity, state.url.clone(), true)
                    }
                };

                if should_start {
                    spawn_download_worker(entity, url);
                }
            }
            DownloadEvent::DismissDialog => {
                world.resource_mut::<DownloadState>().completed_dialog = None;
            }
            DownloadEvent::Tick => {}
            DownloadEvent::ShowSystemDialog { title, description } => {
                spawn_system_dialog(event.entity, title, description);
            }
            DownloadEvent::SystemDialogClosed => {
                let mut state = world.resource_mut::<DownloadState>();
                if !state.in_progress {
                    state.status = "Download complete (system dialog closed).".to_string();
                }
            }
            DownloadEvent::WorkerStarted {
                total_bytes,
                target,
            } => {
                let mut state = world.resource_mut::<DownloadState>();
                state.in_progress = true;
                state.total_bytes = total_bytes;
                state.active_target = Some(target);
                state.status = "Downloading...".to_string();
            }
            DownloadEvent::WorkerProgress {
                downloaded_bytes,
                total_bytes,
            } => {
                let mut state = world.resource_mut::<DownloadState>();
                state.downloaded_bytes = downloaded_bytes;
                state.total_bytes = total_bytes;
                state.status = "Downloading...".to_string();
            }
            DownloadEvent::WorkerFinished { target } => {
                let (use_system_dialog, message) = {
                    let mut state = world.resource_mut::<DownloadState>();
                    state.in_progress = false;
                    state.active_target = Some(target.clone());

                    let message = format!("Saved to: {target}");
                    if state.use_system_dialog {
                        state.completed_dialog = None;
                        state.status = "Download complete. Opening system dialog...".to_string();
                        (true, message)
                    } else {
                        state.status = "Download complete.".to_string();
                        state.completed_dialog = Some(message.clone());
                        (false, message)
                    }
                };

                if use_system_dialog {
                    emit_ui_action(
                        event.entity,
                        DownloadEvent::ShowSystemDialog {
                            title: "Download finished".to_string(),
                            description: message,
                        },
                    );
                }
            }
            DownloadEvent::WorkerFailed(message) => {
                let mut state = world.resource_mut::<DownloadState>();
                state.in_progress = false;
                state.status = format!("Download failed: {message}");
            }
        }
    }
}

fn build_download_app() -> App {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .insert_resource(DownloadState::default())
        .register_projector::<DownloadRootView>(project_download_root)
        .add_systems(Startup, (setup_download_styles, setup_download_world))
        .add_systems(PreUpdate, drain_download_events);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_download_app(), "Download with Progress", |options| {
        options.with_initial_inner_size(LogicalSize::new(760.0, 360.0))
    })
}
