use std::sync::Arc;

use bevy_app::{App, PreUpdate};
use bevy_ecs::{hierarchy::ChildOf, prelude::*};
use bevy_xilem::{
    BevyXilemPlugin, ProjectionCtx, UiEventReceiver, UiEventSender, UiLabel, UiNodeId,
    UiProjectorRegistry, UiRoot, UiView, XilemAction, run_app,
};
use xilem::{
    WindowOptions,
    winit::{dpi::LogicalSize, error::EventLoopError},
};
use xilem_masonry::view::{label, text_button};

#[derive(Debug, Clone, PartialEq, Eq)]
enum CalcEvent {
    Input(String),
    Evaluate,
    Clear,
}

#[derive(Resource, Debug, Default)]
struct CalculatorEngine {
    stored_value: Option<f64>,
    pending_op: Option<char>,
    current_input: String,
    just_evaluated: bool,
    error: bool,
}

impl CalculatorEngine {
    fn clear(&mut self) {
        self.stored_value = None;
        self.pending_op = None;
        self.current_input.clear();
        self.just_evaluated = false;
        self.error = false;
    }

    fn apply_event(&mut self, event: CalcEvent) {
        match event {
            CalcEvent::Input(token) => self.input_token(&token),
            CalcEvent::Evaluate => self.evaluate(),
            CalcEvent::Clear => self.clear(),
        }
    }

    fn input_token(&mut self, token: &str) {
        if self.error {
            self.clear();
        }

        match token {
            "+" | "-" | "*" | "/" => self.push_operator(token.chars().next().unwrap_or('+')),
            "." => {
                if self.just_evaluated && self.pending_op.is_none() {
                    self.stored_value = None;
                    self.just_evaluated = false;
                }
                if self.current_input.is_empty() {
                    self.current_input.push_str("0.");
                } else if !self.current_input.contains('.') {
                    self.current_input.push('.');
                }
            }
            digit if digit.chars().all(|c| c.is_ascii_digit()) => {
                if self.just_evaluated && self.pending_op.is_none() {
                    self.stored_value = None;
                    self.current_input.clear();
                }
                self.just_evaluated = false;

                if self.current_input == "0" {
                    self.current_input = digit.to_string();
                } else {
                    self.current_input.push_str(digit);
                }
            }
            _ => {}
        }
    }

    fn push_operator(&mut self, op: char) {
        self.just_evaluated = false;

        if self.current_input.is_empty() {
            if self.stored_value.is_none() {
                self.stored_value = Some(0.0);
            }
            self.pending_op = Some(op);
            return;
        }

        let rhs = match self.current_input.parse::<f64>() {
            Ok(value) => value,
            Err(_) => {
                self.error = true;
                return;
            }
        };

        let next_value = match (self.stored_value, self.pending_op) {
            (Some(lhs), Some(pending)) => Self::apply_binary(lhs, pending, rhs),
            _ => Some(rhs),
        };

        match next_value {
            Some(value) => {
                self.stored_value = Some(value);
                self.pending_op = Some(op);
                self.current_input.clear();
            }
            None => {
                self.error = true;
            }
        }
    }

    fn evaluate(&mut self) {
        if self.error {
            return;
        }

        if self.current_input.is_empty() {
            return;
        }

        let rhs = match self.current_input.parse::<f64>() {
            Ok(value) => value,
            Err(_) => {
                self.error = true;
                return;
            }
        };

        let result = match (self.stored_value, self.pending_op) {
            (Some(lhs), Some(op)) => Self::apply_binary(lhs, op, rhs),
            _ => Some(rhs),
        };

        match result {
            Some(value) => {
                self.stored_value = Some(value);
                self.pending_op = None;
                self.current_input.clear();
                self.just_evaluated = true;
            }
            None => {
                self.error = true;
            }
        }
    }

    fn apply_binary(lhs: f64, op: char, rhs: f64) -> Option<f64> {
        match op {
            '+' => Some(lhs + rhs),
            '-' => Some(lhs - rhs),
            '*' => Some(lhs * rhs),
            '/' => {
                if rhs.abs() < f64::EPSILON {
                    None
                } else {
                    Some(lhs / rhs)
                }
            }
            _ => None,
        }
    }

    fn display_text(&self) -> String {
        if self.error {
            return "Error".to_string();
        }

        if !self.current_input.is_empty() {
            return self.current_input.clone();
        }

        match self.stored_value {
            Some(value) => format_number(value),
            None => "0".to_string(),
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

#[derive(Component, Debug, Clone, Copy, Default)]
struct CalcDisplay;

#[derive(Component, Debug, Clone, PartialEq, Eq)]
struct CalcButton(String);

#[derive(Resource, Debug, Clone, Copy)]
struct CalcDisplayEntity(Entity);

fn project_calc_display(_: &CalcDisplay, ctx: ProjectionCtx<'_>) -> UiView {
    let text = ctx
        .world
        .get::<UiLabel>(ctx.entity)
        .map_or_else(|| "0".to_string(), |label| label.text.clone());

    Arc::new(label(text))
}

fn calc_event_for_label(label: &str) -> CalcEvent {
    match label {
        "=" => CalcEvent::Evaluate,
        "C" => CalcEvent::Clear,
        other => CalcEvent::Input(other.to_string()),
    }
}

fn project_calc_button(button: &CalcButton, ctx: ProjectionCtx<'_>) -> UiView {
    let sender = ctx.world.resource::<UiEventSender>().0.clone();
    let event = calc_event_for_label(&button.0);

    Arc::new(text_button(button.0.clone(), move |_| {
        let _ = sender.send(XilemAction::action(event.clone()));
    }))
}

fn setup_calculator_world(world: &mut World) {
    let mut next_node_id = 1_u64;
    let mut alloc_node_id = || {
        let id = UiNodeId(next_node_id);
        next_node_id += 1;
        id
    };

    let root = world
        .spawn((UiRoot, alloc_node_id(), bevy_xilem::UiFlexColumn))
        .id();

    let display = world
        .spawn((
            alloc_node_id(),
            CalcDisplay,
            UiLabel::new("0"),
            ChildOf(root),
        ))
        .id();

    world.insert_resource(CalcDisplayEntity(display));

    let grid = world
        .spawn((alloc_node_id(), bevy_xilem::UiFlexColumn, ChildOf(root)))
        .id();

    let layout = [
        ["7", "8", "9", "/"],
        ["4", "5", "6", "*"],
        ["1", "2", "3", "-"],
        ["0", ".", "=", "+"],
    ];

    for row_tokens in layout {
        let row = world
            .spawn((alloc_node_id(), bevy_xilem::UiFlexRow, ChildOf(grid)))
            .id();

        for token in row_tokens {
            world.spawn((alloc_node_id(), CalcButton(token.to_string()), ChildOf(row)));
        }
    }

    world.spawn((alloc_node_id(), CalcButton("C".to_string()), ChildOf(grid)));
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

fn install_projectors(world: &mut World) {
    let mut registry = world.resource_mut::<UiProjectorRegistry>();
    registry
        .register_component::<CalcDisplay>(project_calc_display)
        .register_component::<CalcButton>(project_calc_button);
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
        WindowOptions::new("Bevy Xilem Calculator")
            .with_initial_inner_size(LogicalSize::new(420.0, 640.0)),
    )
}
