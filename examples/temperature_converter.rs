use std::sync::Arc;

use bevy_app::{App, PreUpdate};
use bevy_ecs::prelude::*;
use bevy_xilem::{
    BevyXilemPlugin, ProjectionCtx, UiEventQueue, UiNodeId, UiProjectorRegistry, UiRoot, UiView,
    emit_ui_action, run_app_with_window_options,
};
use xilem::{
    Color,
    masonry::layout::Length,
    masonry::properties::Padding,
    palette,
    style::Style as _,
    view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row, label, text_input},
    winit::{dpi::LogicalSize, error::EventLoopError},
};

/// 7GUIs-like Temperature Converter.
///
/// Two text inputs (Celsius / Fahrenheit) that stay in sync whenever the edited field
/// parses as a number.
#[derive(Resource, Debug, Clone)]
struct TemperatureState {
    celsius_text: String,
    fahrenheit_text: String,
}

impl Default for TemperatureState {
    fn default() -> Self {
        Self {
            celsius_text: "0".to_string(),
            fahrenheit_text: "32".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
enum TemperatureEvent {
    SetCelsiusText(String),
    SetFahrenheitText(String),
}

#[derive(Component, Debug, Clone, Copy)]
struct TemperatureRootView;

fn format_number(value: f64) -> String {
    let mut v = value;
    if v == -0.0 {
        v = 0.0;
    }

    // Keep the formatting stable and human-friendly (avoid long tails).
    let mut text = format!("{v:.10}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text.is_empty() {
        "0".to_string()
    } else {
        text
    }
}

fn parse_number(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

fn c_to_f(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

fn f_to_c(f: f64) -> f64 {
    (f - 32.0) * 5.0 / 9.0
}

fn apply_temperature_event(state: &mut TemperatureState, event: TemperatureEvent) {
    match event {
        TemperatureEvent::SetCelsiusText(new_text) => {
            state.celsius_text = new_text.clone();

            if new_text.trim().is_empty() {
                state.fahrenheit_text.clear();
                return;
            }

            if let Some(c) = parse_number(&new_text) {
                state.fahrenheit_text = format_number(c_to_f(c));
            }
        }
        TemperatureEvent::SetFahrenheitText(new_text) => {
            state.fahrenheit_text = new_text.clone();

            if new_text.trim().is_empty() {
                state.celsius_text.clear();
                return;
            }

            if let Some(f) = parse_number(&new_text) {
                state.celsius_text = format_number(f_to_c(f));
            }
        }
    }
}

fn project_temperature_root(_: &TemperatureRootView, ctx: ProjectionCtx<'_>) -> UiView {
    let state = ctx.world.resource::<TemperatureState>().clone();
    let entity = ctx.entity;

    let title = label("Temperature Converter")
        .text_size(24.0)
        .color(palette::css::WHITE)
        .padding(Padding::bottom(8.0));

    let celsius_row = flex_row((
        text_input(state.celsius_text, move |_, new_value| {
            emit_ui_action(entity, TemperatureEvent::SetCelsiusText(new_value));
        })
        .placeholder("0")
        .text_size(16.0)
        .flex(1.0),
        label("Celsius").text_size(16.0).padding(Padding::left(8.0)),
    ))
    .gap(Length::px(8.0));

    let entity_for_f = ctx.entity;
    let fahrenheit_row = flex_row((
        text_input(state.fahrenheit_text, move |_, new_value| {
            emit_ui_action(entity_for_f, TemperatureEvent::SetFahrenheitText(new_value));
        })
        .placeholder("32")
        .text_size(16.0)
        .flex(1.0),
        label("Fahrenheit")
            .text_size(16.0)
            .padding(Padding::left(8.0)),
    ))
    .gap(Length::px(8.0));

    let hint = label("Tip: 输入无法解析的内容时，另一侧不会被覆盖。")
        .text_size(12.0)
        .color(Color::from_rgb8(0xb0, 0xb0, 0xb0))
        .padding(Padding::top(8.0));

    Arc::new(
        flex_col((title, celsius_row, fahrenheit_row, hint))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(8.0))
            .padding(16.0)
            .background_color(Color::from_rgb8(0x20, 0x20, 0x20))
            .corner_radius(12.0)
            .border(palette::css::DARK_SLATE_GRAY, 1.0),
    )
}

fn install_projectors(world: &mut World) {
    let mut registry = world.resource_mut::<UiProjectorRegistry>();
    registry.register_component::<TemperatureRootView>(project_temperature_root);
}

fn setup_temperature_world(world: &mut World) {
    world.spawn((UiRoot, UiNodeId(1), TemperatureRootView));
}

fn drain_temperature_events(world: &mut World) {
    let events = world
        .resource::<UiEventQueue>()
        .drain_actions::<TemperatureEvent>();
    if events.is_empty() {
        return;
    }

    let mut state = world.resource_mut::<TemperatureState>();
    for event in events {
        apply_temperature_event(&mut state, event.action);
    }
}

fn build_bevy_temperature_app() -> App {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .insert_resource(TemperatureState::default());

    install_projectors(app.world_mut());
    setup_temperature_world(app.world_mut());

    app.add_systems(PreUpdate, drain_temperature_events);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(
        build_bevy_temperature_app(),
        "Temperature Converter",
        |options| options.with_initial_inner_size(LogicalSize::new(520.0, 240.0)),
    )
}
