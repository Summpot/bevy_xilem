use std::sync::Arc;

use bevy_app::{App, PreUpdate};
use bevy_ecs::prelude::*;
use bevy_xilem::{
    BevyXilemPlugin, ProjectionCtx, UiEventQueue, UiNodeId, UiProjectorRegistry, UiRoot, UiView,
    ecs_button_with_child, ecs_text_button, run_app_with_window_options,
};
use xilem::{
    Color,
    masonry::layout::Length,
    palette,
    style::Style as _,
    view::{FlexExt as _, flex_col, flex_row, label},
    winit::{dpi::LogicalSize, error::EventLoopError},
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalcButtonKind {
    Digit,
    Action,
    Operator,
}

#[derive(Debug, Clone)]
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

fn project_calc_button(
    entity: Entity,
    button_data: &CalcButtonSpec,
    highlight_clear_entry: bool,
) -> UiView {
    let event = button_data.event.clone();

    match button_data.kind {
        CalcButtonKind::Digit => Arc::new(
            ecs_text_button(entity, event, button_data.label)
                .background_color(Color::from_rgb8(0x3a, 0x3a, 0x3a))
                .corner_radius(10.0)
                .border_color(Color::TRANSPARENT),
        ),
        CalcButtonKind::Action | CalcButtonKind::Operator => {
            let label_color = if button_data.event == CalcEvent::ClearEntry && highlight_clear_entry
            {
                palette::css::MEDIUM_VIOLET_RED
            } else {
                palette::css::WHITE
            };

            Arc::new(
                ecs_button_with_child(entity, event, label(button_data.label).color(label_color))
                    .background_color(Color::from_rgb8(0x00, 0x8d, 0xdd))
                    .corner_radius(10.0)
                    .border_color(Color::TRANSPARENT)
                    .hovered_border_color(Color::WHITE),
            )
        }
    }
}

fn project_calc_root(_: &CalcRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let engine = ctx.world.resource::<CalculatorEngine>();
    let highlight_clear_entry = engine.current_number().is_empty();

    let mut children = vec![
        flex_row((label(engine.display_text()).text_size(30.0),))
            .padding(8.0)
            .border(palette::css::DARK_SLATE_GRAY, 1.0)
            .into_any_flex(),
    ];

    for row in calc_button_rows() {
        let row_children = row
            .iter()
            .map(|button_data| {
                project_calc_button(ctx.entity, button_data, highlight_clear_entry).into_any_flex()
            })
            .collect::<Vec<_>>();

        children.push(flex_row(row_children).gap(Length::px(2.)).into_any_flex());
    }

    Arc::new(flex_col(children).gap(Length::px(2.)).padding(12.0))
}

fn install_projectors(world: &mut World) {
    let mut registry = world.resource_mut::<UiProjectorRegistry>();
    registry.register_component::<CalcRoot>(project_calc_root);
}

fn setup_calculator_world(world: &mut World) {
    world.spawn((UiRoot, UiNodeId(1), CalcRoot));
}

fn drain_calc_events(world: &mut World) {
    let events = world
        .resource::<UiEventQueue>()
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
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .insert_resource(CalculatorEngine::default());

    install_projectors(app.world_mut());
    setup_calculator_world(app.world_mut());

    app.add_systems(PreUpdate, drain_calc_events);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_bevy_calculator_app(), "Calculator", |options| {
        options.with_initial_inner_size(LogicalSize::new(400.0, 500.0))
    })
}
