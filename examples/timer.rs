use std::{sync::Arc, time::Instant};

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ColorStyle, LayoutStyle, ProjectionCtx, StyleClass,
    StyleRule, StyleSheet, TextStyle, UiEventQueue, UiRoot, UiView, apply_label_style,
    apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::prelude::*,
    button_with_child, resolve_style, resolve_style_for_classes, run_app_with_window_options,
    slider,
    xilem::{
        view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row, label, progress_bar},
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};

/// 7GUIs-like Timer.
///
/// - Shows elapsed time
/// - Progress bar (elapsed / duration)
/// - Duration can be adjusted while running
/// - Reset button
#[derive(Resource, Debug, Clone)]
struct TimerState {
    duration_secs: f64,
    elapsed_secs: f64,
    last_tick: Instant,
}

impl Default for TimerState {
    fn default() -> Self {
        Self {
            duration_secs: 10.0,
            elapsed_secs: 0.0,
            last_tick: Instant::now(),
        }
    }
}

#[derive(Debug, Clone)]
enum TimerEvent {
    SetDurationSecs(f64),
    Reset,
}

#[derive(Component, Debug, Clone, Copy)]
struct TimerRootView;

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn format_secs(secs: f64) -> String {
    // Keep it readable (one decimal place like many 7GUIs implementations).
    format!("{secs:.1} s")
}

fn apply_timer_event(state: &mut TimerState, event: TimerEvent) {
    match event {
        TimerEvent::SetDurationSecs(new_duration) => {
            state.duration_secs = new_duration.max(0.1);
            state.elapsed_secs = state.elapsed_secs.min(state.duration_secs);
        }
        TimerEvent::Reset => {
            state.elapsed_secs = 0.0;
            state.last_tick = Instant::now();
        }
    }
}

fn tick_timer(state: &mut TimerState) {
    let now = Instant::now();
    let dt = now.saturating_duration_since(state.last_tick).as_secs_f64();
    state.last_tick = now;

    if state.elapsed_secs < state.duration_secs {
        state.elapsed_secs = (state.elapsed_secs + dt).min(state.duration_secs);
    }
}

fn project_timer_root(_: &TimerRootView, ctx: ProjectionCtx<'_>) -> UiView {
    let root_style = resolve_style(ctx.world, ctx.entity);
    let title_style = resolve_style_for_classes(ctx.world, ["timer.title"]);
    let row_style = resolve_style_for_classes(ctx.world, ["timer.row"]);
    let body_text_style = resolve_style_for_classes(ctx.world, ["timer.body-text"]);
    let reset_button_style = resolve_style_for_classes(ctx.world, ["timer.reset-button"]);
    let reset_label_style = resolve_style_for_classes(ctx.world, ["timer.reset-label"]);

    let state = ctx.world.resource::<TimerState>().clone();

    let progress = if state.duration_secs > 0.0 {
        Some(clamp01(state.elapsed_secs / state.duration_secs))
    } else {
        Some(1.0)
    };

    let title = apply_label_style(label("Timer"), &title_style);

    let elapsed_row = apply_widget_style(
        flex_row((
            apply_label_style(label("Elapsed Time:"), &body_text_style),
            apply_label_style(label(format_secs(state.elapsed_secs)), &body_text_style),
        )),
        &row_style,
    );

    let duration_value = state.duration_secs;
    let duration_row = apply_widget_style(
        flex_row((
            apply_label_style(
                label(format!("Duration: {duration_value:.0} s")),
                &body_text_style,
            ),
            slider(
                ctx.entity,
                1.0,
                60.0,
                duration_value,
                TimerEvent::SetDurationSecs,
            )
            .step(1.0)
            .flex(1.0),
        )),
        &row_style,
    );

    let reset = apply_widget_style(
        button_with_child(
            ctx.entity,
            TimerEvent::Reset,
            apply_label_style(label("Reset"), &reset_label_style),
        ),
        &reset_button_style,
    );

    Arc::new(apply_widget_style(
        flex_col((
            title,
            elapsed_row,
            progress_bar(progress),
            duration_row,
            reset,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start),
        &root_style,
    ))
}

fn setup_timer_world(mut commands: Commands) {
    commands.spawn((
        UiRoot,
        TimerRootView,
        StyleClass(vec!["timer.root".to_string()]),
    ));
}

fn setup_timer_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "timer.root",
        StyleRule {
            layout: LayoutStyle {
                padding: Some(16.0),
                gap: Some(10.0),
                corner_radius: Some(12.0),
                border_width: Some(1.0),
            },
            colors: ColorStyle {
                bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x20, 0x20, 0x20)),
                border: Some(bevy_xilem::xilem::palette::css::DARK_SLATE_GRAY),
                ..ColorStyle::default()
            },
            ..StyleRule::default()
        },
    );

    style_sheet.set_class(
        "timer.title",
        StyleRule {
            text: TextStyle { size: Some(24.0) },
            colors: ColorStyle {
                text: Some(bevy_xilem::xilem::palette::css::WHITE),
                ..ColorStyle::default()
            },
            ..StyleRule::default()
        },
    );

    style_sheet.set_class(
        "timer.row",
        StyleRule {
            layout: LayoutStyle {
                gap: Some(8.0),
                ..LayoutStyle::default()
            },
            ..StyleRule::default()
        },
    );

    style_sheet.set_class(
        "timer.body-text",
        StyleRule {
            text: TextStyle { size: Some(16.0) },
            layout: LayoutStyle {
                padding: Some(4.0),
                ..LayoutStyle::default()
            },
            ..StyleRule::default()
        },
    );

    style_sheet.set_class(
        "timer.reset-button",
        StyleRule {
            layout: LayoutStyle {
                padding: Some(6.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x35, 0x35, 0x35)),
                border: Some(bevy_xilem::xilem::palette::css::DARK_SLATE_GRAY),
                ..ColorStyle::default()
            },
            ..StyleRule::default()
        },
    );

    style_sheet.set_class(
        "timer.reset-label",
        StyleRule {
            text: TextStyle { size: Some(16.0) },
            colors: ColorStyle {
                text: Some(bevy_xilem::xilem::palette::css::WHITE),
                ..ColorStyle::default()
            },
            ..StyleRule::default()
        },
    );
}

fn drain_timer_events_and_tick(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<TimerEvent>();

    {
        let mut state = world.resource_mut::<TimerState>();
        for event in events {
            apply_timer_event(&mut state, event.action);
        }
        tick_timer(&mut state);
    }
}

fn build_bevy_timer_app() -> App {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .insert_resource(TimerState::default())
        .register_projector::<TimerRootView>(project_timer_root)
        .add_systems(Startup, (setup_timer_styles, setup_timer_world));

    app.add_systems(PreUpdate, drain_timer_events_and_tick);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_bevy_timer_app(), "Timer", |options| {
        options.with_initial_inner_size(LogicalSize::new(520.0, 260.0))
    })
}
