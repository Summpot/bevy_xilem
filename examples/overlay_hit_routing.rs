mod utils;

use std::sync::Arc;

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, BuiltinUiAction, ProjectionCtx, UiButton, UiComboBox,
    UiComboOption, UiEventQueue, UiFlexColumn, UiLabel, UiRoot, UiView,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    run_app_with_window_options, spawn_in_overlay_root,
    xilem::{
        view::{label, transformed},
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};
use utils::init_logging;

#[derive(Component, Debug, Clone)]
struct UiToast {
    message: String,
}

#[derive(Component, Debug, Clone, Copy)]
struct SpawnToastButton;

fn project_ui_toast(toast: &UiToast, _ctx: ProjectionCtx<'_>) -> UiView {
    Arc::new(transformed(label(toast.message.clone())).translate((520.0, 40.0)))
}

fn setup_overlay_hit_routing_world(mut commands: Commands) {
    let root = commands.spawn((UiRoot, UiFlexColumn)).id();

    commands.spawn((
        UiLabel::new(
            "Open the dropdown, spawn a toast, then click the toast.\n\
             Expected: dropdown closes immediately; toast stays visible.",
        ),
        ChildOf(root),
    ));

    commands.spawn((
        UiComboBox::new(vec![
            UiComboOption::new("alpha", "Alpha"),
            UiComboOption::new("beta", "Beta"),
            UiComboOption::new("gamma", "Gamma"),
        ])
        .with_placeholder("Open dropdown"),
        ChildOf(root),
    ));

    commands.spawn((
        UiButton::new("Spawn Toast"),
        SpawnToastButton,
        ChildOf(root),
    ));
}

fn drain_overlay_hit_routing_events(world: &mut World) {
    let button_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<BuiltinUiAction>();

    if button_events.is_empty() {
        return;
    }

    for event in button_events {
        if !matches!(event.action, BuiltinUiAction::Clicked) {
            continue;
        }

        if world.get::<SpawnToastButton>(event.entity).is_none() {
            continue;
        }

        let has_toast = {
            let mut query = world.query_filtered::<Entity, With<UiToast>>();
            query.iter(world).next().is_some()
        };

        if has_toast {
            continue;
        }

        spawn_in_overlay_root(
            world,
            (UiToast {
                message: "🍞 Toast: I am outside OverlayStack logic.".to_string(),
            },),
        );
    }
}

fn build_overlay_hit_routing_app() -> App {
    init_logging();

    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .load_style_sheet("themes/default_theme.ron")
        .register_projector::<UiToast>(project_ui_toast)
        .add_systems(Startup, setup_overlay_hit_routing_world)
        .add_systems(PreUpdate, drain_overlay_hit_routing_events);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_overlay_hit_routing_app(), "Overlay Hit Routing", |opts| {
        opts.with_initial_inner_size(LogicalSize::new(960.0, 640.0))
    })
}
