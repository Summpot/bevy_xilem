use std::sync::Arc;

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ColorStyle, LayoutStyle, ProjectionCtx, StyleClass,
    StyleSetter, StyleSheet, TextStyle, UiEventQueue, UiRoot, UiView, apply_label_style,
    apply_text_input_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::prelude::*,
    resolve_style, resolve_style_for_classes, run_app_with_window_options, text_input,
    xilem::{
        view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row, label},
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
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
    let root_style = resolve_style(ctx.world, ctx.entity);
    let title_style = resolve_style_for_classes(ctx.world, ["temp.title"]);
    let row_style = resolve_style_for_classes(ctx.world, ["temp.row"]);
    let unit_label_style = resolve_style_for_classes(ctx.world, ["temp.unit-label"]);
    let input_style = resolve_style_for_classes(ctx.world, ["temp.input"]);
    let hint_style = resolve_style_for_classes(ctx.world, ["temp.hint"]);

    let state = ctx.world.resource::<TemperatureState>().clone();

    let title = apply_label_style(label("Temperature Converter"), &title_style);

    let celsius_row = apply_widget_style(
        flex_row((
            apply_text_input_style(
                text_input(
                    ctx.entity,
                    state.celsius_text,
                    TemperatureEvent::SetCelsiusText,
                )
                .placeholder("0"),
                &input_style,
            )
            .flex(1.0),
            apply_label_style(label("Celsius"), &unit_label_style),
        )),
        &row_style,
    );

    let fahrenheit_row = apply_widget_style(
        flex_row((
            apply_text_input_style(
                text_input(
                    ctx.entity,
                    state.fahrenheit_text,
                    TemperatureEvent::SetFahrenheitText,
                )
                .placeholder("32"),
                &input_style,
            )
            .flex(1.0),
            apply_label_style(label("Fahrenheit"), &unit_label_style),
        )),
        &row_style,
    );

    let hint = apply_label_style(
        label("Tip: invalid numeric input will not overwrite the other field."),
        &hint_style,
    );

    Arc::new(apply_widget_style(
        flex_col((title, celsius_row, fahrenheit_row, hint))
            .cross_axis_alignment(CrossAxisAlignment::Start),
        &root_style,
    ))
}

fn setup_temperature_world(mut commands: Commands) {
    commands.spawn((
        UiRoot,
        TemperatureRootView,
        StyleClass(vec!["temp.root".to_string()]),
    ));
}

fn setup_temperature_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "temp.root",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(16.0),
                gap: Some(8.0),
                corner_radius: Some(12.0),
                border_width: Some(1.0),
            },
            colors: ColorStyle {
                bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x20, 0x20, 0x20)),
                border: Some(bevy_xilem::xilem::palette::css::DARK_SLATE_GRAY),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "temp.title",
        StyleSetter {
            text: TextStyle { size: Some(24.0) },
            colors: ColorStyle {
                text: Some(bevy_xilem::xilem::palette::css::WHITE),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "temp.row",
        StyleSetter {
            layout: LayoutStyle {
                gap: Some(8.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "temp.unit-label",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            layout: LayoutStyle {
                padding: Some(8.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "temp.input",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "temp.hint",
        StyleSetter {
            text: TextStyle { size: Some(12.0) },
            colors: ColorStyle {
                text: Some(bevy_xilem::xilem::Color::from_rgb8(0xb0, 0xb0, 0xb0)),
                ..ColorStyle::default()
            },
            layout: LayoutStyle {
                padding: Some(8.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );
}

fn drain_temperature_events(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
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
        .insert_resource(TemperatureState::default())
        .register_projector::<TemperatureRootView>(project_temperature_root)
        .add_systems(Startup, (setup_temperature_styles, setup_temperature_world));

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
