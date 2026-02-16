use std::sync::Arc;

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ProjectionCtx, UiEventQueue, UiRoot, UiView,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::{
        hierarchy::{ChildOf, Children},
        prelude::*,
    },
    button_with_child, checkbox, emit_ui_action, run_app, text_button, text_input,
    xilem::{
        InsertNewline,
        masonry::{
            layout::Length,
            theme::{DEFAULT_GAP, ZYNC_800},
        },
        style::Style as _,
        view::{
            FlexExt as _, FlexSpacer, MainAxisAlignment, flex_col, flex_row, label, sized_box,
            virtual_scroll,
        },
        winit::error::EventLoopError,
    },
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
    let entity_for_enter = ctx.entity;

    Arc::new(
        flex_row((
            text_input(ctx.entity, draft, TodoEvent::SetDraft)
                .text_size(16.0)
                .placeholder("What needs to be done?")
                .insert_newline(InsertNewline::OnShiftEnter)
                .on_enter(move |_, _| {
                    emit_ui_action(entity_for_enter, TodoEvent::SubmitDraft);
                })
                .flex(1.0),
            button_with_child(
                ctx.entity,
                TodoEvent::SubmitDraft,
                label("Add task").text_size(16.0),
            ),
        ))
        .gap(DEFAULT_GAP),
    )
}

fn project_todo_list_container(_: &TodoListContainer, ctx: ProjectionCtx<'_>) -> UiView {
    let active_filter = ctx.world.resource::<ActiveFilter>().0;
    let child_entities = ctx
        .world
        .get::<Children>(ctx.entity)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();

    let visible_children = child_entities
        .into_iter()
        .zip(ctx.children)
        .filter_map(|(entity, child)| {
            let item = ctx.world.get::<TodoItem>(entity)?;
            todo_matches_filter(item, active_filter).then_some(child)
        })
        .collect::<Vec<_>>();

    if visible_children.is_empty() {
        return Arc::new(
            label("No tasks for this filter.")
                .padding(8.0)
                .border(ZYNC_800, 1.0),
        );
    }

    let visible_children = Arc::new(visible_children);
    let item_count = i64::try_from(visible_children.len()).unwrap_or(i64::MAX);

    Arc::new(
        sized_box(virtual_scroll(0..item_count, {
            let visible_children = Arc::clone(&visible_children);
            move |_, idx| {
                let index = usize::try_from(idx).expect("virtual scroll index should be positive");
                visible_children
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| Arc::new(label("")))
            }
        }))
        .fixed_height(Length::px(360.0))
        .padding(4.0)
        .border(ZYNC_800, 1.0),
    )
}

fn project_todo_item(item: &TodoItem, ctx: ProjectionCtx<'_>) -> UiView {
    let entity = ctx.entity;

    Arc::new(
        flex_row((
            checkbox(entity, item.text.clone(), item.completed, move |value| {
                TodoEvent::SetCompleted(entity, value)
            })
            .text_size(16.0),
            FlexSpacer::Flex(1.0),
            text_button(entity, TodoEvent::Delete(entity), "Delete").padding(5.0),
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
        return Arc::new(label(""));
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

    Arc::new(checkbox(
        ctx.entity,
        filter.as_str(),
        active == filter,
        move |_| TodoEvent::SetFilter(filter),
    ))
}

fn todo_matches_filter(item: &TodoItem, filter: FilterType) -> bool {
    match filter {
        FilterType::All => true,
        FilterType::Active => !item.completed,
        FilterType::Completed => item.completed,
    }
}

fn spawn_todo_item(world: &mut World, text: String, done: bool) -> Entity {
    let list_container = world.resource::<TodoRuntime>().list_container;
    world
        .spawn((
            TodoItem {
                text,
                completed: done,
            },
            ChildOf(list_container),
        ))
        .id()
}

fn setup_todo_world(mut commands: Commands) {
    let root = commands.spawn((UiRoot, TodoRootView)).id();

    commands.spawn((TodoHeader, ChildOf(root)));
    commands.spawn((TodoInputArea, ChildOf(root)));

    let list_container = commands.spawn((TodoListContainer, ChildOf(root))).id();

    let footer_bar = commands.spawn((TodoFilterBar, ChildOf(root))).id();
    commands.spawn((FilterToggle(FilterType::All), ChildOf(footer_bar)));
    commands.spawn((FilterToggle(FilterType::Active), ChildOf(footer_bar)));
    commands.spawn((FilterToggle(FilterType::Completed), ChildOf(footer_bar)));

    commands.insert_resource(TodoRuntime { list_container });

    for i in 1..=120 {
        commands.spawn((
            TodoItem {
                text: format!("Sample task #{i}"),
                completed: i % 3 == 0,
            },
            ChildOf(list_container),
        ));
    }
}

fn drain_todo_events_and_mutate_world(world: &mut World) {
    let events = world
        .resource::<UiEventQueue>()
        .drain_actions::<TodoEvent>();
    if events.is_empty() {
        return;
    }

    for event in events {
        match event.action {
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
        .insert_resource(DraftTodo("My Next Task".to_string()))
        .register_projector::<TodoRootView>(project_todo_root)
        .register_projector::<TodoHeader>(project_todo_header)
        .register_projector::<TodoInputArea>(project_todo_input_area)
        .register_projector::<TodoListContainer>(project_todo_list_container)
        .register_projector::<TodoItem>(project_todo_item)
        .register_projector::<TodoFilterBar>(project_filter_bar)
        .register_projector::<FilterToggle>(project_filter_toggle)
        .add_systems(Startup, setup_todo_world);

    app.add_systems(PreUpdate, drain_todo_events_and_mutate_world);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app(build_bevy_todo_app(), "To Do MVC")
}
