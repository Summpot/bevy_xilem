# bevy_xilem

`bevy_xilem` connects **Bevy ECS** with a **retained Xilem/Masonry UI runtime**.

You describe UI from ECS components (via projectors), handle user interactions through a typed queue, and let `bevy_xilem` synthesize/rebuild the widget tree each frame.

---

## Features

- Bevy-first update loop and scheduling
- ECS-driven UI projection (`Component -> UiView`)
- Typed UI action queue (`UiEventQueue`) for interaction handling
- Ergonomic ECS control helpers (`button`, `checkbox`, `slider`, `text_input`, ...)
- Built-in i18n/l10n bridge via `bevy_fluent` (`ActiveLocale` + `LocalizeText`)
- Windowed runner helpers for apps/examples

---

## Installation

Add only `bevy_xilem` to your dependencies:

```toml
[dependencies]
bevy_xilem = "0.1"
```

If you are using this repository workspace layout, keep path dependencies from the workspace root.

---

## Quick start

```rust,no_run
use std::sync::Arc;

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ProjectionCtx, UiEventQueue, UiRoot, UiView,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::prelude::*,
    run_app_with_window_options, text_button,
    xilem::winit::{dpi::LogicalSize, error::EventLoopError},
};

#[derive(Component, Debug, Clone, Copy)]
struct CounterRoot;

#[derive(Resource, Debug, Default)]
struct Counter(i32);

#[derive(Debug, Clone, Copy)]
enum CounterEvent {
    Increment,
}

fn project_counter_root(_: &CounterRoot, ctx: ProjectionCtx<'_>) -> UiView {
    Arc::new(text_button(ctx.entity, CounterEvent::Increment, "Increment"))
}

fn setup(mut commands: Commands) {
    commands.spawn((UiRoot, CounterRoot));
}

fn drain_events(world: &mut World) {
    let events = world
        .resource::<UiEventQueue>()
        .drain_actions::<CounterEvent>();

    if events.is_empty() {
        return;
    }

    let mut counter = world.resource_mut::<Counter>();
    for _ in events {
        counter.0 += 1;
    }
}

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .insert_resource(Counter::default())
        .register_projector::<CounterRoot>(project_counter_root)
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, drain_events);
    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_app(), "Counter", |options| {
        options.with_initial_inner_size(LogicalSize::new(360.0, 220.0))
    })
}
```

---

## Styling system (brief)

`bevy_xilem` includes an ECS-driven, CSS-like styling pipeline:

- define class rules in `StyleSheet`
- attach classes to entities with `StyleClass`
- resolve/apply styles in projectors with helper functions
- configure hover/pressed + transition colors for smooth interaction feedback

Minimal setup sketch:

```rust,no_run
use bevy_xilem::{
    ColorStyle, LayoutStyle, StyleClass, StyleSetter, StyleSheet, StyleTransition, TextStyle,
    apply_label_style, apply_widget_style, resolve_style,
    bevy_ecs::prelude::*,
    xilem::{Color, view::{flex_col, label}},
};

fn setup_styles(mut sheet: ResMut<StyleSheet>) {
    sheet.set_class(
        "demo.button",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                corner_radius: Some(8.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x25, 0x63, 0xEB)),
                hover_bg: Some(Color::from_rgb8(0x1D, 0x4E, 0xD8)),
                pressed_bg: Some(Color::from_rgb8(0x1E, 0x40, 0xAF)),
                text: Some(Color::WHITE),
                ..ColorStyle::default()
            },
            text: TextStyle { size: Some(16.0) },
            transition: Some(StyleTransition { duration: 0.15 }),
        },
    );
}

fn project_demo(entity: Entity, world: &World) -> impl bevy_xilem::xilem_masonry::WidgetView<(), ()> {
    let style = resolve_style(world, entity);
    apply_widget_style(
        flex_col((apply_label_style(label("Hello styled UI"), &style),)),
        &style,
    )
}

fn spawn_demo(mut commands: Commands) {
    commands.spawn(StyleClass(vec!["demo.button".to_string()]));
}
```

For the full guide (cascade rules, projector patterns, transitions, caveats), see [`STYLING.md`](./STYLING.md).

---

## Reusable custom view helper

You can wrap repeated UI patterns as reusable helper functions that return a typed view.

```rust,no_run
use bevy_ecs::entity::Entity;
use bevy_xilem::{button_with_child, xilem::view::label};

#[derive(Debug, Clone)]
enum TodoAction {
    Add,
    Remove,
}

fn accent_action_button(
    entity: Entity,
    action: TodoAction,
    text: &'static str,
) -> impl bevy_xilem::xilem_masonry::WidgetView<(), ()> {
    button_with_child(entity, action, label(text))
        .padding(8.0)
        .corner_radius(10.0)
        .background_color(bevy_xilem::xilem::Color::from_rgb8(0x00, 0x8d, 0xdd))
}
```

Use it in projectors just like built-in controls:

```rust,no_run
# use std::sync::Arc;
# use bevy_xilem::{ProjectionCtx, UiView};
# #[derive(Debug, Clone)] enum TodoAction { Add }
# fn accent_action_button(
#     entity: bevy_xilem::bevy_ecs::entity::Entity,
#     action: TodoAction,
#     text: &'static str,
# ) -> impl bevy_xilem::xilem_masonry::WidgetView<(), ()> {
#     bevy_xilem::button_with_child(entity, action, bevy_xilem::xilem::view::label(text))
# }
fn project_toolbar(ctx: ProjectionCtx<'_>) -> UiView {
    Arc::new(accent_action_button(ctx.entity, TodoAction::Add, "Add task"))
}
```

---

## API naming conventions

`bevy_xilem` exports two control groups:

- **ECS action adapters** (recommended): `button`, `checkbox`, `slider`, `switch`, `text_button`, `text_input`
- **Original xilem controls** with `xilem_` prefix: `xilem_button`, `xilem_checkbox`, ...

Legacy `ecs_*` names are still available for compatibility.

---

## Event handling model

1. Controls emit typed actions into `UiEventQueue`
2. A Bevy system drains typed actions in `PreUpdate`
3. Your app mutates ECS state/resources
4. `bevy_xilem` synthesizes and rebuilds UI in `PostUpdate`

This keeps interaction handling explicit and ECS-friendly.

---

## Included examples

- `calculator`
- `temperature_converter`
- `timer`
- `todo_list`
- `chess`
- `theme_gallery`
- `game_2048`
- `i18n_showcase`

Run an example from repository root:

```bash
cargo run --example todo_list
```

---

## License

Dual-licensed under MIT OR Apache-2.0.
