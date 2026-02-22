mod utils;

use std::sync::Arc;

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ColorStyle, LayoutStyle, ProjectionCtx, StyleClass,
    StyleSetter, StyleSheet, StyleTransition, TextStyle, UiEventQueue, UiRoot, UiView,
    apply_label_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    button, resolve_style, resolve_style_for_classes, run_app_with_window_options,
    xilem::{
        view::{FlexExt as _, flex_col, flex_row, label},
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};
use utils::init_logging;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MathOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl MathOperator {
    fn as_str(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "−",
            Self::Multiply => "×",
            Self::Divide => "÷",
        }
    }

    fn perform_op(self, num1: f64, num2: f64) -> f64 {
        match self {
            Self::Add => num1 + num2,
            Self::Subtract => num1 - num2,
            Self::Multiply => num1 * num2,
            Self::Divide => num1 / num2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum CalcEvent {
    Digit(String),
    Operator(MathOperator),
    Equals,
    ClearEntry,
    ClearAll,
    Delete,
    Negate,
}

#[derive(Resource, Debug, Default)]
struct CalculatorEngine {
    current_num_index: usize,
    clear_current_entry_on_input: bool,
    numbers: [String; 2],
    result: Option<String>,
    operation: Option<MathOperator>,
}

impl CalculatorEngine {
    fn current_number(&self) -> &str {
        &self.numbers[self.current_num_index]
    }

    fn current_number_owned(&self) -> String {
        self.current_number().to_string()
    }

    fn set_current_number(&mut self, new_num: String) {
        self.numbers[self.current_num_index] = new_num;
    }

    fn clear_all(&mut self) {
        self.current_num_index = 0;
        self.result = None;
        self.operation = None;
        self.clear_current_entry_on_input = false;
        for number in &mut self.numbers {
            *number = String::new();
        }
    }

    fn clear_entry(&mut self) {
        self.clear_current_entry_on_input = false;
        if self.result.is_some() {
            self.clear_all();
            return;
        }
        self.set_current_number(String::new());
    }

    fn on_entered_digit(&mut self, digit: &str) {
        if self.result.is_some() {
            self.clear_all();
        } else if self.clear_current_entry_on_input {
            self.clear_entry();
        }

        let mut number = self.current_number_owned();
        if digit == "." {
            if number.contains('.') {
                return;
            }
            if number.is_empty() {
                number = "0".into();
            }
            number.push('.');
        } else if number == "0" || number.is_empty() {
            number = digit.to_string();
        } else {
            number.push_str(digit);
        }

        self.set_current_number(number);
    }

    fn on_entered_operator(&mut self, operator: MathOperator) {
        self.clear_current_entry_on_input = false;

        if self.operation.is_some() && !self.numbers[1].is_empty() {
            if self.result.is_none() {
                self.on_equals();
            }
            self.move_result_to_left();
            self.current_num_index = 1;
        } else if self.current_num_index == 0 {
            if self.numbers[0].is_empty() {
                return;
            }
            self.current_num_index = 1;
        }

        self.operation = Some(operator);
    }

    fn move_result_to_left(&mut self) {
        self.clear_current_entry_on_input = true;
        self.numbers[0] = self.result.clone().unwrap_or_default();
        self.numbers[1].clear();
        self.operation = None;
        self.current_num_index = 0;
        self.result = None;
    }

    fn on_equals(&mut self) {
        if self.numbers[0].is_empty() || self.numbers[1].is_empty() {
            return;
        }

        if self.result.is_some() {
            self.numbers[0] = self.result.clone().unwrap_or_default();
        }

        self.current_num_index = 0;

        let num1 = self.numbers[0].parse::<f64>();
        let num2 = self.numbers[1].parse::<f64>();

        self.result = Some(match (num1, num2, self.operation) {
            (Ok(lhs), Ok(rhs), Some(op)) => format_number(op.perform_op(lhs, rhs)),
            (Err(err), _, _) => err.to_string(),
            (_, Err(err), _) => err.to_string(),
            (_, _, None) => self.numbers[0].clone(),
        });
    }

    fn on_delete(&mut self) {
        if self.result.is_some() {
            return;
        }

        let mut number = self.current_number_owned();
        if !number.is_empty() {
            number.pop();
            self.set_current_number(number);
        }
    }

    fn negate(&mut self) {
        if self.result.is_some() {
            self.move_result_to_left();
        }

        let mut number = self.current_number_owned();
        if number.is_empty() {
            return;
        }

        if number.starts_with('-') {
            number.remove(0);
        } else {
            number = format!("-{number}");
        }

        self.set_current_number(number);
    }

    fn apply_event(&mut self, event: CalcEvent) {
        match event {
            CalcEvent::Digit(digit) => self.on_entered_digit(&digit),
            CalcEvent::Operator(operator) => self.on_entered_operator(operator),
            CalcEvent::Equals => self.on_equals(),
            CalcEvent::ClearEntry => self.clear_entry(),
            CalcEvent::ClearAll => self.clear_all(),
            CalcEvent::Delete => self.on_delete(),
            CalcEvent::Negate => self.negate(),
        }
    }

    fn display_text(&self) -> String {
        let mut fragments = Vec::new();

        if !self.numbers[0].is_empty() {
            fragments.push(self.numbers[0].clone());
        }
        if let Some(operation) = self.operation {
            fragments.push(operation.as_str().to_string());
        }
        if !self.numbers[1].is_empty() {
            fragments.push(self.numbers[1].clone());
        }
        if let Some(result) = &self.result {
            fragments.push("=".to_string());
            fragments.push(result.clone());
        }

        if fragments.is_empty() {
            "0".to_string()
        } else {
            fragments.join(" ")
        }
    }
}

fn format_number(value: f64) -> String {
    let mut text = format!("{value:.10}");
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

#[derive(Component, Debug, Clone, Copy)]
struct CalcRoot;

#[derive(Component, Debug, Clone, Copy)]
struct CalcDisplayPanel;

#[derive(Component, Debug, Clone, Copy)]
struct CalcKeypad;

#[derive(Component, Debug, Clone, Copy)]
struct CalcButtonRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalcButtonKind {
    Digit,
    Action,
    Operator,
}

#[derive(Component, Debug, Clone)]
struct CalcButtonSpec {
    label: &'static str,
    event: CalcEvent,
    kind: CalcButtonKind,
}

fn calc_button_rows() -> Vec<Vec<CalcButtonSpec>> {
    vec![
        vec![
            CalcButtonSpec {
                label: "CE",
                event: CalcEvent::ClearEntry,
                kind: CalcButtonKind::Action,
            },
            CalcButtonSpec {
                label: "C",
                event: CalcEvent::ClearAll,
                kind: CalcButtonKind::Action,
            },
            CalcButtonSpec {
                label: "DEL",
                event: CalcEvent::Delete,
                kind: CalcButtonKind::Action,
            },
            CalcButtonSpec {
                label: "÷",
                event: CalcEvent::Operator(MathOperator::Divide),
                kind: CalcButtonKind::Operator,
            },
        ],
        vec![
            CalcButtonSpec {
                label: "7",
                event: CalcEvent::Digit("7".into()),
                kind: CalcButtonKind::Digit,
            },
            CalcButtonSpec {
                label: "8",
                event: CalcEvent::Digit("8".into()),
                kind: CalcButtonKind::Digit,
            },
            CalcButtonSpec {
                label: "9",
                event: CalcEvent::Digit("9".into()),
                kind: CalcButtonKind::Digit,
            },
            CalcButtonSpec {
                label: "×",
                event: CalcEvent::Operator(MathOperator::Multiply),
                kind: CalcButtonKind::Operator,
            },
        ],
        vec![
            CalcButtonSpec {
                label: "4",
                event: CalcEvent::Digit("4".into()),
                kind: CalcButtonKind::Digit,
            },
            CalcButtonSpec {
                label: "5",
                event: CalcEvent::Digit("5".into()),
                kind: CalcButtonKind::Digit,
            },
            CalcButtonSpec {
                label: "6",
                event: CalcEvent::Digit("6".into()),
                kind: CalcButtonKind::Digit,
            },
            CalcButtonSpec {
                label: "−",
                event: CalcEvent::Operator(MathOperator::Subtract),
                kind: CalcButtonKind::Operator,
            },
        ],
        vec![
            CalcButtonSpec {
                label: "1",
                event: CalcEvent::Digit("1".into()),
                kind: CalcButtonKind::Digit,
            },
            CalcButtonSpec {
                label: "2",
                event: CalcEvent::Digit("2".into()),
                kind: CalcButtonKind::Digit,
            },
            CalcButtonSpec {
                label: "3",
                event: CalcEvent::Digit("3".into()),
                kind: CalcButtonKind::Digit,
            },
            CalcButtonSpec {
                label: "+",
                event: CalcEvent::Operator(MathOperator::Add),
                kind: CalcButtonKind::Operator,
            },
        ],
        vec![
            CalcButtonSpec {
                label: "±",
                event: CalcEvent::Negate,
                kind: CalcButtonKind::Action,
            },
            CalcButtonSpec {
                label: "0",
                event: CalcEvent::Digit("0".into()),
                kind: CalcButtonKind::Digit,
            },
            CalcButtonSpec {
                label: ".",
                event: CalcEvent::Digit(".".into()),
                kind: CalcButtonKind::Action,
            },
            CalcButtonSpec {
                label: "=",
                event: CalcEvent::Equals,
                kind: CalcButtonKind::Action,
            },
        ],
    ]
}

fn project_calc_button(entity: Entity, button_data: &CalcButtonSpec, world: &World) -> UiView {
    let event = button_data.event.clone();

    let button_class = match button_data.kind {
        CalcButtonKind::Digit => "calc.button.digit",
        CalcButtonKind::Action => "calc.button.action",
        CalcButtonKind::Operator => "calc.button.operator",
    };

    let button_style = resolve_style_for_classes(world, [button_class]);

    Arc::new(apply_widget_style(
        button(entity, event, button_data.label),
        &button_style,
    ))
}

fn project_calc_root(_: &CalcRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let root_style = resolve_style(ctx.world, ctx.entity);
    Arc::new(apply_widget_style(
        flex_col(
            ctx.children
                .into_iter()
                .map(|child| child.into_any_flex())
                .collect::<Vec<_>>(),
        ),
        &root_style,
    ))
}

fn project_calc_display(_: &CalcDisplayPanel, ctx: ProjectionCtx<'_>) -> UiView {
    let display_row_style = resolve_style_for_classes(ctx.world, ["calc.display.row"]);
    let display_text_style = resolve_style_for_classes(ctx.world, ["calc.display.text"]);
    let engine = ctx.world.resource::<CalculatorEngine>();

    Arc::new(apply_widget_style(
        flex_row((
            apply_label_style(label(engine.display_text()), &display_text_style).into_any_flex(),
        )),
        &display_row_style,
    ))
}

fn project_calc_keypad(_: &CalcKeypad, ctx: ProjectionCtx<'_>) -> UiView {
    Arc::new(flex_col(
        ctx.children
            .into_iter()
            .map(|child| child.into_any_flex())
            .collect::<Vec<_>>(),
    ))
}

fn project_calc_row(_: &CalcButtonRow, ctx: ProjectionCtx<'_>) -> UiView {
    let row_style = resolve_style_for_classes(ctx.world, ["calc.row"]);
    Arc::new(apply_widget_style(
        flex_row(
            ctx.children
                .into_iter()
                .map(|child| child.into_any_flex())
                .collect::<Vec<_>>(),
        ),
        &row_style,
    ))
}

fn project_calc_button_component(button_data: &CalcButtonSpec, ctx: ProjectionCtx<'_>) -> UiView {
    project_calc_button(ctx.entity, button_data, ctx.world)
}

fn setup_calculator_world(mut commands: Commands) {
    let root = commands
        .spawn((UiRoot, CalcRoot, StyleClass(vec!["calc.root".to_string()])))
        .id();
    commands.spawn((CalcDisplayPanel, ChildOf(root)));

    let keypad = commands.spawn((CalcKeypad, ChildOf(root))).id();
    for row in calc_button_rows() {
        let row_entity = commands.spawn((CalcButtonRow, ChildOf(keypad))).id();
        for button_spec in row {
            commands.spawn((button_spec, ChildOf(row_entity)));
        }
    }
}

fn setup_calculator_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "calc.root",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(12.0),
                gap: Some(2.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "calc.display.row",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                border: Some(bevy_xilem::xilem::palette::css::DARK_SLATE_GRAY),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "calc.display.text",
        StyleSetter {
            text: TextStyle { size: Some(30.0) },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "calc.row",
        StyleSetter {
            layout: LayoutStyle {
                gap: Some(2.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "calc.button.digit",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(10.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x3a, 0x3a, 0x3a)),
                hover_bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x4a, 0x4a, 0x4a)),
                pressed_bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x2e, 0x2e, 0x2e)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.15 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "calc.button.action",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(10.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x00, 0x8d, 0xdd)),
                hover_bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x00, 0x7b, 0xc2)),
                pressed_bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x00, 0x64, 0x9c)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.15 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "calc.button.operator",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(10.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x00, 0x8d, 0xdd)),
                hover_bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x00, 0x7b, 0xc2)),
                pressed_bg: Some(bevy_xilem::xilem::Color::from_rgb8(0x00, 0x64, 0x9c)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.15 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "calc.button.label.default",
        StyleSetter {
            text: TextStyle { size: Some(18.0) },
            colors: ColorStyle {
                text: Some(bevy_xilem::xilem::palette::css::WHITE),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "calc.button.label.clear",
        StyleSetter {
            text: TextStyle { size: Some(18.0) },
            colors: ColorStyle {
                text: Some(bevy_xilem::xilem::palette::css::MEDIUM_VIOLET_RED),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );
}

fn drain_calc_events(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<CalcEvent>();
    if events.is_empty() {
        return;
    }

    let mut engine = world.resource_mut::<CalculatorEngine>();
    for event in events {
        engine.apply_event(event.action);
    }
}

fn build_bevy_calculator_app() -> App {
    init_logging();

    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .load_style_sheet("themes/default_theme.ron")
        .insert_resource(CalculatorEngine::default())
        .register_projector::<CalcRoot>(project_calc_root)
        .register_projector::<CalcDisplayPanel>(project_calc_display)
        .register_projector::<CalcKeypad>(project_calc_keypad)
        .register_projector::<CalcButtonRow>(project_calc_row)
        .register_projector::<CalcButtonSpec>(project_calc_button_component)
        .add_systems(Startup, (setup_calculator_styles, setup_calculator_world));

    app.add_systems(PreUpdate, drain_calc_events);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_bevy_calculator_app(), "Calculator", |options| {
        options.with_initial_inner_size(LogicalSize::new(400.0, 500.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_xilem::bevy_ecs::schedule::Schedule;

    #[test]
    fn setup_spawns_componentized_keypad_entities() {
        let mut world = World::new();
        let mut schedule = Schedule::default();
        schedule.add_systems(setup_calculator_world);
        schedule.run(&mut world);

        let mut row_query = world.query::<&CalcButtonRow>();
        let mut key_query = world.query::<&CalcButtonSpec>();

        assert_eq!(row_query.iter(&world).count(), 5);
        assert_eq!(key_query.iter(&world).count(), 20);
    }
}
