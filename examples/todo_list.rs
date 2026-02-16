use std::sync::Arc;

use bevy_app::{App, PreUpdate};
use bevy_ecs::{
    hierarchy::{ChildOf, Children},
    prelude::*,
};
use bevy_xilem::{
    BevyXilemPlugin, ProjectionCtx, UiEventReceiver, UiNodeId, UiProjectorRegistry, UiRoot, UiView,
    XilemAction, run_app,
};
use xilem::{
    InsertNewline, WindowOptions,
    masonry::{
        layout::Length,
        theme::{DEFAULT_GAP, ZYNC_800},
    },
    style::Style as _,
    view::{
        FlexExt as _, FlexSpacer, MainAxisAlignment, button, checkbox, flex_col, flex_row, label,
        text_button, text_input,
    },
    winit::error::EventLoopError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FilterType {
    All,
    Active,
    Completed,
}

impl FilterType {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Active => "Active",
            Self::Completed => "Completed",
        }
    }
}

#[derive(Debug, Clone)]
enum TodoEvent {
    SetDraft(String),
    SubmitDraft,
    SetCompleted(Entity, bool),
    Delete(Entity),
    SetFilter(FilterType),
}

#[derive(Resource, Debug, Clone)]
struct DraftTodo(String);

#[derive(Resource, Debug, Clone, Copy)]
struct ActiveFilter(FilterType);

#[derive(Resource, Debug, Clone, Copy)]
struct TodoRuntime {
    list_container: Entity,
    next_node_id: u64,
}

#[derive(Component, Debug, Clone, Copy)]
struct TodoRootView;

#[derive(Component, Debug, Clone, Copy)]
struct TodoHeader;

#[derive(Component, Debug, Clone, Copy)]
struct TodoInputArea;

#[derive(Component, Debug, Clone, Copy)]
struct TodoListContainer;

#[derive(Component, Debug, Clone)]
struct TodoItem {
    text: String,
    completed: bool,
}

#[derive(Component, Debug, Clone, Copy)]
struct TodoFilterBar;

#[derive(Component, Debug, Clone, Copy)]
struct FilterToggle(FilterType);

fn alloc_node_id(world: &mut World) -> UiNodeId {
    let mut runtime = world.resource_mut::<TodoRuntime>();
    let id = UiNodeId(runtime.next_node_id);
    runtime.next_node_id += 1;
    id
}

fn project_todo_root(_: &TodoRootView, ctx: ProjectionCtx<'_>) -> UiView {
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(flex_col(children).gap(Length::px(4.)).padding(50.0))
}

fn project_todo_header(_: &TodoHeader, _: ProjectionCtx<'_>) -> UiView {
    Arc::new(label("todos").text_size(80.0))
}

fn project_todo_input_area(_: &TodoInputArea, ctx: ProjectionCtx<'_>) -> UiView {
    let draft = ctx.world.resource::<DraftTodo>().0.clone();
    let sender = ctx.event_sender.clone();
    let sender_for_enter = sender.clone();
    let sender_for_button = sender.clone();

    Arc::new(
        flex_row((
            text_input(draft, move |_, new_value| {
                let _ = sender.send(XilemAction::action(TodoEvent::SetDraft(new_value)));
            })
            .text_size(16.0)
            .placeholder("What needs to be done?")
            .insert_newline(InsertNewline::OnShiftEnter)
            .on_enter(move |_, _| {
                let _ = sender_for_enter.send(XilemAction::action(TodoEvent::SubmitDraft));
            })
            .flex(1.0),
            button(label("Add task").text_size(16.0), move |_| {
                let _ = sender_for_button.send(XilemAction::action(TodoEvent::SubmitDraft));
            }),
        ))
        .gap(DEFAULT_GAP),
    )
}

fn project_todo_list_container(_: &TodoListContainer, ctx: ProjectionCtx<'_>) -> UiView {
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(flex_col(children).gap(DEFAULT_GAP))
}

fn project_todo_item(item: &TodoItem, ctx: ProjectionCtx<'_>) -> UiView {
    let should_show = match ctx.world.resource::<ActiveFilter>().0 {
        FilterType::All => true,
        FilterType::Active => !item.completed,
        FilterType::Completed => item.completed,
    };

    if !should_show {
        return Arc::new(flex_row(()));
    }

    let entity = ctx.entity;
    let sender = ctx.event_sender.clone();
    let sender_for_delete = sender.clone();

    Arc::new(
        flex_row((
            checkbox(item.text.clone(), item.completed, move |_, value| {
                let _ = sender.send(XilemAction::action(TodoEvent::SetCompleted(entity, value)));
            })
            .text_size(16.0),
            FlexSpacer::Flex(1.0),
            text_button("Delete", move |_| {
                let _ = sender_for_delete.send(XilemAction::action(TodoEvent::Delete(entity)));
            })
            .padding(5.0),
        ))
        .padding(DEFAULT_GAP.get())
        .border(ZYNC_800, 1.0),
    )
}

fn project_filter_bar(_: &TodoFilterBar, ctx: ProjectionCtx<'_>) -> UiView {
    let list_container = ctx.world.resource::<TodoRuntime>().list_container;
    let has_tasks = ctx
        .world
        .get::<Children>(list_container)
        .is_some_and(|children| !children.is_empty());

    if !has_tasks {
        return Arc::new(flex_row(()));
    }

    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(flex_row(children).main_axis_alignment(MainAxisAlignment::Center))
}

fn project_filter_toggle(filter_toggle: &FilterToggle, ctx: ProjectionCtx<'_>) -> UiView {
    let filter = filter_toggle.0;
    let active = ctx.world.resource::<ActiveFilter>().0;
    let sender = ctx.event_sender.clone();

    Arc::new(checkbox(filter.as_str(), active == filter, move |_, _| {
        let _ = sender.send(XilemAction::action(TodoEvent::SetFilter(filter)));
    }))
}

fn install_projectors(world: &mut World) {
    let mut registry = world.resource_mut::<UiProjectorRegistry>();
    registry
        .register_component::<TodoRootView>(project_todo_root)
        .register_component::<TodoHeader>(project_todo_header)
        .register_component::<TodoInputArea>(project_todo_input_area)
        .register_component::<TodoListContainer>(project_todo_list_container)
        .register_component::<TodoItem>(project_todo_item)
        .register_component::<TodoFilterBar>(project_filter_bar)
        .register_component::<FilterToggle>(project_filter_toggle);
}

fn spawn_todo_item(world: &mut World, text: String, done: bool) -> Entity {
    let list_container = world.resource::<TodoRuntime>().list_container;
    let node_id = alloc_node_id(world);
    world
        .spawn((
            node_id,
            TodoItem {
                text,
                completed: done,
            },
            ChildOf(list_container),
        ))
        .id()
}

fn setup_todo_world(world: &mut World) {
    let mut next_node_id = 1_u64;
    let mut alloc = || {
        let id = UiNodeId(next_node_id);
        next_node_id += 1;
        id
    };

    let root = world.spawn((UiRoot, alloc(), TodoRootView)).id();

    world.spawn((alloc(), TodoHeader, ChildOf(root)));
    world.spawn((alloc(), TodoInputArea, ChildOf(root)));

    let list_container = world
        .spawn((alloc(), TodoListContainer, ChildOf(root)))
        .id();

    let footer_bar = world.spawn((alloc(), TodoFilterBar, ChildOf(root))).id();
    world.spawn((alloc(), FilterToggle(FilterType::All), ChildOf(footer_bar)));
    world.spawn((
        alloc(),
        FilterToggle(FilterType::Active),
        ChildOf(footer_bar),
    ));
    world.spawn((
        alloc(),
        FilterToggle(FilterType::Completed),
        ChildOf(footer_bar),
    ));

    world.insert_resource(TodoRuntime {
        list_container,
        next_node_id,
    });

    spawn_todo_item(world, "Buy milk".to_string(), false);
    spawn_todo_item(world, "Buy eggs".to_string(), true);
    spawn_todo_item(world, "Buy bread".to_string(), false);
}

fn drain_todo_events_and_mutate_world(world: &mut World) {
    let events = world
        .resource::<UiEventReceiver>()
        .drain_actions::<TodoEvent>();
    if events.is_empty() {
        return;
    }

    for event in events {
        match event {
            TodoEvent::SetDraft(text) => {
                world.resource_mut::<DraftTodo>().0 = text;
            }
            TodoEvent::SubmitDraft => {
                let text = {
                    let mut draft = world.resource_mut::<DraftTodo>();
                    let text = draft.0.trim().to_string();
                    if !text.is_empty() {
                        draft.0.clear();
                    }
                    text
                };

                if !text.is_empty() {
                    spawn_todo_item(world, text, false);
                }
            }
            TodoEvent::SetCompleted(entity, done) => {
                if let Some(mut todo) = world.get_mut::<TodoItem>(entity) {
                    todo.completed = done;
                }
            }
            TodoEvent::Delete(entity) => {
                if world.get_entity(entity).is_ok() {
                    world.entity_mut(entity).despawn();
                }
            }
            TodoEvent::SetFilter(filter) => {
                world.resource_mut::<ActiveFilter>().0 = filter;
            }
        }
    }
}

fn build_bevy_todo_app() -> App {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .insert_resource(ActiveFilter(FilterType::All))
        .insert_resource(DraftTodo("My Next Task".to_string()));

    install_projectors(app.world_mut());
    setup_todo_world(app.world_mut());

    app.add_systems(PreUpdate, drain_todo_events_and_mutate_world);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app(build_bevy_todo_app(), WindowOptions::new("To Do MVC"))
}
