use std::sync::Arc;

use bevy_app::{App, PreUpdate};
use bevy_ecs::{hierarchy::ChildOf, prelude::*};
use bevy_xilem::{
    BevyXilemPlugin, ProjectionCtx, UiEventReceiver, UiEventSender, UiLabel, UiNodeId,
    UiProjectorRegistry, UiRoot, UiView, XilemAction, run_app,
};
use xilem::{
    Color, WindowOptions,
    masonry::layout::Length,
    palette,
    style::Style as _,
    view::{FlexExt as _, button, flex_col, flex_row, label, text_button},
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

    fn clear_entry_hint_color(&self) -> Color {
        if self.current_number().is_empty() {
            palette::css::MEDIUM_VIOLET_RED
        } else {
            palette::css::WHITE
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
struct CalcDisplayRow;

#[derive(Component, Debug, Clone, Copy)]
struct CalcButtonRow;

#[derive(Component, Debug, Clone)]
struct CalcButton {
    label: String,
    event: CalcEvent,
    kind: CalcButtonKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalcButtonKind {
    Digit,
    Action,
    Operator,
}

#[derive(Resource, Debug, Clone, Copy)]
struct CalcDisplayEntity(Entity);

fn project_calc_root(_: &CalcRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(flex_col(children).gap(Length::px(2.)).padding(12.0))
}

fn project_calc_display_row(_: &CalcDisplayRow, ctx: ProjectionCtx<'_>) -> UiView {
    let text = ctx
        .world
        .get::<UiLabel>(ctx.entity)
        .map_or_else(|| "0".to_string(), |label| label.text.clone());

    Arc::new(
        flex_row((label(text).text_size(30.0),))
            .padding(8.0)
            .border(palette::css::DARK_SLATE_GRAY, 1.0),
    )
}

fn project_calc_button_row(_: &CalcButtonRow, ctx: ProjectionCtx<'_>) -> UiView {
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(flex_row(children).gap(Length::px(2.)))
}

fn project_calc_button(button_data: &CalcButton, ctx: ProjectionCtx<'_>) -> UiView {
    let sender = ctx.world.resource::<UiEventSender>().0.clone();
    let event = button_data.event.clone();

    match button_data.kind {
        CalcButtonKind::Digit => Arc::new(
            text_button(button_data.label.clone(), move |_| {
                let _ = sender.send(XilemAction::action(event.clone()));
            })
            .background_color(Color::from_rgb8(0x3a, 0x3a, 0x3a))
            .corner_radius(10.0)
            .border_color(Color::TRANSPARENT),
        ),
        CalcButtonKind::Action | CalcButtonKind::Operator => {
            let ce_color = if button_data.event == CalcEvent::ClearEntry {
                ctx.world.get_resource::<CalculatorEngine>().map_or(
                    palette::css::WHITE,
                    CalculatorEngine::clear_entry_hint_color,
                )
            } else {
                palette::css::WHITE
            };

            Arc::new(
                button(
                    label(button_data.label.clone()).color(ce_color),
                    move |_| {
                        let _ = sender.send(XilemAction::action(event.clone()));
                    },
                )
                .background_color(Color::from_rgb8(0x00, 0x8d, 0xdd))
                .corner_radius(10.0)
                .border_color(Color::TRANSPARENT)
                .hovered_border_color(Color::WHITE),
            )
        }
    }
}

fn install_projectors(world: &mut World) {
    let mut registry = world.resource_mut::<UiProjectorRegistry>();
    registry
        .register_component::<CalcRoot>(project_calc_root)
        .register_component::<CalcDisplayRow>(project_calc_display_row)
        .register_component::<CalcButtonRow>(project_calc_button_row)
        .register_component::<CalcButton>(project_calc_button);
}

fn spawn_calc_button(
    world: &mut World,
    parent: Entity,
    alloc_node_id: &mut impl FnMut() -> UiNodeId,
    label: &str,
    event: CalcEvent,
    kind: CalcButtonKind,
) {
    world.spawn((
        alloc_node_id(),
        CalcButton {
            label: label.to_string(),
            event,
            kind,
        },
        ChildOf(parent),
    ));
}

fn setup_calculator_world(world: &mut World) {
    let mut next_node_id = 1_u64;
    let mut alloc_node_id = || {
        let id = UiNodeId(next_node_id);
        next_node_id += 1;
        id
    };

    let root = world.spawn((UiRoot, alloc_node_id(), CalcRoot)).id();

    let display = world
        .spawn((
            alloc_node_id(),
            CalcDisplayRow,
            UiLabel::new("0"),
            ChildOf(root),
        ))
        .id();
    world.insert_resource(CalcDisplayEntity(display));

    let top = world
        .spawn((alloc_node_id(), CalcButtonRow, ChildOf(root)))
        .id();
    spawn_calc_button(
        world,
        top,
        &mut alloc_node_id,
        "CE",
        CalcEvent::ClearEntry,
        CalcButtonKind::Action,
    );
    spawn_calc_button(
        world,
        top,
        &mut alloc_node_id,
        "C",
        CalcEvent::ClearAll,
        CalcButtonKind::Action,
    );
    spawn_calc_button(
        world,
        top,
        &mut alloc_node_id,
        "DEL",
        CalcEvent::Delete,
        CalcButtonKind::Action,
    );
    spawn_calc_button(
        world,
        top,
        &mut alloc_node_id,
        "÷",
        CalcEvent::Operator(MathOperator::Divide),
        CalcButtonKind::Operator,
    );

    let rows = [
        [
            ("7", CalcEvent::Digit("7".into()), CalcButtonKind::Digit),
            ("8", CalcEvent::Digit("8".into()), CalcButtonKind::Digit),
            ("9", CalcEvent::Digit("9".into()), CalcButtonKind::Digit),
            (
                "×",
                CalcEvent::Operator(MathOperator::Multiply),
                CalcButtonKind::Operator,
            ),
        ],
        [
            ("4", CalcEvent::Digit("4".into()), CalcButtonKind::Digit),
            ("5", CalcEvent::Digit("5".into()), CalcButtonKind::Digit),
            ("6", CalcEvent::Digit("6".into()), CalcButtonKind::Digit),
            (
                "−",
                CalcEvent::Operator(MathOperator::Subtract),
                CalcButtonKind::Operator,
            ),
        ],
        [
            ("1", CalcEvent::Digit("1".into()), CalcButtonKind::Digit),
            ("2", CalcEvent::Digit("2".into()), CalcButtonKind::Digit),
            ("3", CalcEvent::Digit("3".into()), CalcButtonKind::Digit),
            (
                "+",
                CalcEvent::Operator(MathOperator::Add),
                CalcButtonKind::Operator,
            ),
        ],
        [
            ("±", CalcEvent::Negate, CalcButtonKind::Action),
            ("0", CalcEvent::Digit("0".into()), CalcButtonKind::Digit),
            (".", CalcEvent::Digit(".".into()), CalcButtonKind::Action),
            ("=", CalcEvent::Equals, CalcButtonKind::Action),
        ],
    ];

    for row_spec in rows {
        let row = world
            .spawn((alloc_node_id(), CalcButtonRow, ChildOf(root)))
            .id();

        for (label, event, kind) in row_spec {
            spawn_calc_button(world, row, &mut alloc_node_id, label, event, kind);
        }
    }
}

fn drain_calc_events_and_update_display(world: &mut World) {
    let events = world
        .resource::<UiEventReceiver>()
        .drain_actions::<CalcEvent>();
    if events.is_empty() {
        return;
    }

    {
        let mut engine = world.resource_mut::<CalculatorEngine>();
        for event in events {
            engine.apply_event(event);
        }
    }

    let display_text = world.resource::<CalculatorEngine>().display_text();
    let display_entity = world.resource::<CalcDisplayEntity>().0;

    if let Some(mut label) = world.get_mut::<UiLabel>(display_entity) {
        label.text = display_text;
    }
}

fn build_bevy_calculator_app() -> App {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .insert_resource(CalculatorEngine::default());

    install_projectors(app.world_mut());
    setup_calculator_world(app.world_mut());

    app.add_systems(PreUpdate, drain_calc_events_and_update_display);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app(
        build_bevy_calculator_app(),
        WindowOptions::new("Calculator").with_initial_inner_size(LogicalSize::new(400.0, 500.0)),
    )
}
