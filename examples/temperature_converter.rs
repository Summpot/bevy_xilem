mod utils;

use std::sync::Arc;

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ColorStyle, LayoutStyle, ProjectionCtx, StyleClass,
    StyleSetter, StyleSheet, TextStyle, UiEventQueue, UiRoot, UiView, apply_label_style,
    apply_text_input_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    resolve_style, resolve_style_for_classes, run_app_with_window_options, text_input,
    xilem::{
        view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row, label},
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};
use utils::init_logging;

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

#[derive(Component, Debug, Clone, Copy)]
struct TemperatureTitle;

#[derive(Debug, Clone, Copy)]
enum TempScale {
    Celsius,
    Fahrenheit,
}

#[derive(Component, Debug, Clone, Copy)]
struct TemperatureInputRow {
    scale: TempScale,
}

#[derive(Component, Debug, Clone, Copy)]
struct TemperatureHint;

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

fn temperature_input_row_view(
    entity: Entity,
    input_text: String,
    map_event: impl Fn(String) -> TemperatureEvent + Send + Sync + 'static,
    placeholder: &'static str,
    unit_label: &'static str,
    row_style: &bevy_xilem::ResolvedStyle,
    input_style: &bevy_xilem::ResolvedStyle,
    unit_label_style: &bevy_xilem::ResolvedStyle,
) -> UiView {
    Arc::new(apply_widget_style(
        flex_row((
            apply_text_input_style(
                text_input(entity, input_text, map_event).placeholder(placeholder),
                input_style,
            )
            .flex(1.0),
            apply_label_style(label(unit_label), unit_label_style),
        )),
        row_style,
    ))
}

fn project_temperature_root(_: &TemperatureRootView, ctx: ProjectionCtx<'_>) -> UiView {
    let root_style = resolve_style(ctx.world, ctx.entity);

    Arc::new(apply_widget_style(
        flex_col(
            ctx.children
                .into_iter()
                .map(|child| child.into_any_flex())
                .collect::<Vec<_>>(),
        )
        .cross_axis_alignment(CrossAxisAlignment::Start),
        &root_style,
    ))
}

fn project_temperature_title(_: &TemperatureTitle, ctx: ProjectionCtx<'_>) -> UiView {
    let title_style = resolve_style_for_classes(ctx.world, ["temp.title"]);
    Arc::new(apply_label_style(
        label("Temperature Converter"),
        &title_style,
    ))
}

fn project_temperature_input_row(row: &TemperatureInputRow, ctx: ProjectionCtx<'_>) -> UiView {
    let row_style = resolve_style_for_classes(ctx.world, ["temp.row"]);
    let unit_label_style = resolve_style_for_classes(ctx.world, ["temp.unit-label"]);
    let input_style = resolve_style_for_classes(ctx.world, ["temp.input"]);
    let state = ctx.world.resource::<TemperatureState>();

    match row.scale {
        TempScale::Celsius => temperature_input_row_view(
            ctx.entity,
            state.celsius_text.clone(),
            TemperatureEvent::SetCelsiusText,
            "0",
            "Celsius",
            &row_style,
            &input_style,
            &unit_label_style,
        ),
        TempScale::Fahrenheit => temperature_input_row_view(
            ctx.entity,
            state.fahrenheit_text.clone(),
            TemperatureEvent::SetFahrenheitText,
            "32",
            "Fahrenheit",
            &row_style,
            &input_style,
            &unit_label_style,
        ),
    }
}

fn project_temperature_hint(_: &TemperatureHint, ctx: ProjectionCtx<'_>) -> UiView {
    let hint_style = resolve_style_for_classes(ctx.world, ["temp.hint"]);
    Arc::new(apply_label_style(
        label("Tip: invalid numeric input will not overwrite the other field."),
        &hint_style,
    ))
}

fn setup_temperature_world(mut commands: Commands) {
    let root = commands
        .spawn((
            UiRoot,
            TemperatureRootView,
            StyleClass(vec!["temp.root".to_string()]),
        ))
        .id();

    commands.spawn((TemperatureTitle, ChildOf(root)));
    commands.spawn((
        TemperatureInputRow {
            scale: TempScale::Celsius,
        },
        ChildOf(root),
    ));
    commands.spawn((
        TemperatureInputRow {
            scale: TempScale::Fahrenheit,
        },
        ChildOf(root),
    ));
    commands.spawn((TemperatureHint, ChildOf(root)));
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
    init_logging();

    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .register_ui_control::<bevy_xilem::UiButton>()
        .register_ui_control::<bevy_xilem::UiTextInput>()
        .load_style_sheet("themes/default_theme.ron")
        .insert_resource(TemperatureState::default())
        .register_projector::<TemperatureRootView>(project_temperature_root)
        .register_projector::<TemperatureTitle>(project_temperature_title)
        .register_projector::<TemperatureInputRow>(project_temperature_input_row)
        .register_projector::<TemperatureHint>(project_temperature_hint)
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
