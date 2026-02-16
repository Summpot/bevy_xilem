use std::{sync::Arc, time::Instant};

use bevy_app::{App, PreUpdate};
use bevy_ecs::prelude::*;
use bevy_xilem::{
    BevyXilemPlugin, ProjectionCtx, UiEventReceiver, UiNodeId, UiProjectorRegistry, UiRoot, UiView,
    XilemAction, run_app,
};
use xilem::{
    Color, WindowOptions,
    masonry::layout::Length,
    masonry::properties::Padding,
    palette,
    style::Style as _,
    view::{
        CrossAxisAlignment, FlexExt as _, flex_col, flex_row, label, progress_bar, slider,
        text_button,
    },
    winit::{dpi::LogicalSize, error::EventLoopError},
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
            // Keep duration sane and non-zero.
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
    let state = ctx.world.resource::<TimerState>().clone();
    let sender = ctx.event_sender.clone();
    let sender_for_reset = sender.clone();

    let progress = if state.duration_secs > 0.0 {
        Some(clamp01(state.elapsed_secs / state.duration_secs))
    } else {
        Some(1.0)
    };

    let title = label("Timer")
        .text_size(24.0)
        .color(palette::css::WHITE)
        .padding(Padding::bottom(8.0));

    let elapsed_row = flex_row((
        label("Elapsed Time:").text_size(16.0),
        label(format_secs(state.elapsed_secs))
            .text_size(16.0)
            .padding(Padding::left(8.0)),
    ))
    .gap(Length::px(8.0));

    let duration_value = state.duration_secs;
    let duration_row = flex_row((
        label(format!("Duration: {duration_value:.0} s"))
            .text_size(16.0)
            .padding(Padding::top(6.0)),
        slider(1.0, 60.0, duration_value, move |_, val| {
            let _ = sender.send(XilemAction::action(TimerEvent::SetDurationSecs(val)));
        })
        .step(1.0)
        .flex(1.0),
    ))
    .gap(Length::px(8.0));

    let reset = text_button("Reset", move |_| {
        let _ = sender_for_reset.send(XilemAction::action(TimerEvent::Reset));
    })
    .padding(Padding::top(8.0));

    Arc::new(
        flex_col((
            title,
            elapsed_row,
            progress_bar(progress),
            duration_row,
            reset,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(10.0))
        .padding(16.0)
        .background_color(Color::from_rgb8(0x20, 0x20, 0x20))
        .corner_radius(12.0)
        .border(palette::css::DARK_SLATE_GRAY, 1.0),
    )
}

fn install_projectors(world: &mut World) {
    let mut registry = world.resource_mut::<UiProjectorRegistry>();
    registry.register_component::<TimerRootView>(project_timer_root);
}

fn setup_timer_world(world: &mut World) {
    world.spawn((UiRoot, UiNodeId(1), TimerRootView));
}

fn drain_timer_events_and_tick(world: &mut World) {
    let events = world
        .resource::<UiEventReceiver>()
        .drain_actions::<TimerEvent>();

    {
        let mut state = world.resource_mut::<TimerState>();
        for event in events {
            apply_timer_event(&mut state, event);
        }
        tick_timer(&mut state);
    }
}

fn build_bevy_timer_app() -> App {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .insert_resource(TimerState::default());

    install_projectors(app.world_mut());
    setup_timer_world(app.world_mut());

    app.add_systems(PreUpdate, drain_timer_events_and_tick);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app(
        build_bevy_timer_app(),
        WindowOptions::new("Timer").with_initial_inner_size(LogicalSize::new(520.0, 260.0)),
    )
}
