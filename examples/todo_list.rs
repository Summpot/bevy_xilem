use std::sync::Arc;

use bevy_app::{App, PreUpdate};
use bevy_ecs::{hierarchy::ChildOf, prelude::*};
use bevy_xilem::{
    BevyXilemPlugin, ProjectionCtx, UiEventReceiver, UiEventSender, UiNodeId, UiProjectorRegistry,
    UiRoot, UiView, XilemAction, run_app,
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
    root: Entity,
    list_container: Entity,
    footer_bar: Entity,
    next_node_id: u64,
}

#[derive(Component, Debug, Clone, Copy)]
struct TodoRootView;

#[derive(Component, Debug, Clone, Copy)]
struct TodoHeader;

#[derive(Component, Debug, Clone, Copy)]
struct TodoInputLine;

#[derive(Component, Debug, Clone, Copy)]
struct TodoInputField;

#[derive(Component, Debug, Clone, Copy)]
struct AddTodoButton;

#[derive(Component, Debug, Clone, Copy)]
struct TodoListContainer;

#[derive(Component, Debug, Clone, Copy)]
struct TodoItemRow;

#[derive(Component, Debug, Clone)]
struct TodoText(String);

#[derive(Component, Debug, Clone, Copy)]
struct Completed(bool);

#[derive(Component, Debug, Clone, Copy)]
struct ToggleTodoCheckbox {
    target: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
struct DeleteTodoButton {
    target: Entity,
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

fn project_todo_input_line(_: &TodoInputLine, ctx: ProjectionCtx<'_>) -> UiView {
    let mut child_iter = ctx.children.into_iter();
    let mut children = Vec::new();

    if let Some(input) = child_iter.next() {
        children.push(input.flex(1.0).into_any_flex());
    }
    children.extend(child_iter.map(|child| child.into_any_flex()));

    Arc::new(flex_row(children).gap(DEFAULT_GAP))
}

fn project_todo_input_field(_: &TodoInputField, ctx: ProjectionCtx<'_>) -> UiView {
    let draft = ctx.world.resource::<DraftTodo>().0.clone();
    let sender = ctx.world.resource::<UiEventSender>().0.clone();
    let sender_for_enter = sender.clone();

    Arc::new(
        text_input(draft, move |_, new_value| {
            let _ = sender.send(XilemAction::action(TodoEvent::SetDraft(new_value)));
        })
        .text_size(16.0)
        .placeholder("What needs to be done?")
        .insert_newline(InsertNewline::OnShiftEnter)
        .on_enter(move |_, _| {
            let _ = sender_for_enter.send(XilemAction::action(TodoEvent::SubmitDraft));
        }),
    )
}

fn project_add_todo_button(_: &AddTodoButton, ctx: ProjectionCtx<'_>) -> UiView {
    let sender = ctx.world.resource::<UiEventSender>().0.clone();

    Arc::new(button(label("Add task").text_size(16.0), move |_| {
        let _ = sender.send(XilemAction::action(TodoEvent::SubmitDraft));
    }))
}

fn project_todo_list_container(_: &TodoListContainer, ctx: ProjectionCtx<'_>) -> UiView {
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(flex_col(children).gap(DEFAULT_GAP))
}

fn project_todo_item_row(_: &TodoItemRow, ctx: ProjectionCtx<'_>) -> UiView {
    let mut children_iter = ctx.children.into_iter();
    let mut children = Vec::new();

    if let Some(first) = children_iter.next() {
        children.push(first.into_any_flex());
    }
    children.push(FlexSpacer::Flex(1.0).into_any_flex());
    children.extend(children_iter.map(|child| child.into_any_flex()));

    Arc::new(
        flex_row(children)
            .padding(DEFAULT_GAP.get())
            .border(ZYNC_800, 1.0),
    )
}

fn project_toggle_todo_checkbox(toggle: &ToggleTodoCheckbox, ctx: ProjectionCtx<'_>) -> UiView {
    let target = toggle.target;
    let text = ctx.world.get::<TodoText>(target).map_or_else(
        || "<missing todo>".to_string(),
        |todo_text| todo_text.0.clone(),
    );
    let checked = ctx
        .world
        .get::<Completed>(target)
        .is_some_and(|completed| completed.0);

    let sender = ctx.world.resource::<UiEventSender>().0.clone();
    Arc::new(
        checkbox(text, checked, move |_, value| {
            let _ = sender.send(XilemAction::action(TodoEvent::SetCompleted(target, value)));
        })
        .text_size(16.0),
    )
}

fn project_delete_todo_button(delete: &DeleteTodoButton, ctx: ProjectionCtx<'_>) -> UiView {
    let target = delete.target;
    let sender = ctx.world.resource::<UiEventSender>().0.clone();

    Arc::new(
        text_button("Delete", move |_| {
            let _ = sender.send(XilemAction::action(TodoEvent::Delete(target)));
        })
        .padding(5.0),
    )
}

fn project_filter_bar(_: &TodoFilterBar, ctx: ProjectionCtx<'_>) -> UiView {
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
    let sender = ctx.world.resource::<UiEventSender>().0.clone();

    Arc::new(checkbox(filter.as_str(), active == filter, move |_, _| {
        let _ = sender.send(XilemAction::action(TodoEvent::SetFilter(filter)));
    }))
}

fn install_projectors(world: &mut World) {
    let mut registry = world.resource_mut::<UiProjectorRegistry>();
    registry
        .register_component::<TodoRootView>(project_todo_root)
        .register_component::<TodoHeader>(project_todo_header)
        .register_component::<TodoInputLine>(project_todo_input_line)
        .register_component::<TodoInputField>(project_todo_input_field)
        .register_component::<AddTodoButton>(project_add_todo_button)
        .register_component::<TodoListContainer>(project_todo_list_container)
        .register_component::<TodoItemRow>(project_todo_item_row)
        .register_component::<ToggleTodoCheckbox>(project_toggle_todo_checkbox)
        .register_component::<DeleteTodoButton>(project_delete_todo_button)
        .register_component::<TodoFilterBar>(project_filter_bar)
        .register_component::<FilterToggle>(project_filter_toggle);
}

fn spawn_todo_item(world: &mut World, text: String, done: bool) -> Entity {
    let list_container = world.resource::<TodoRuntime>().list_container;
    let row_node_id = alloc_node_id(world);
    let toggle_node_id = alloc_node_id(world);
    let delete_node_id = alloc_node_id(world);

    let row = world
        .spawn((
            row_node_id,
            TodoItemRow,
            TodoText(text),
            Completed(done),
            ChildOf(list_container),
        ))
        .id();

    world.spawn((
        toggle_node_id,
        ToggleTodoCheckbox { target: row },
        ChildOf(row),
    ));
    world.spawn((
        delete_node_id,
        DeleteTodoButton { target: row },
        ChildOf(row),
    ));

    row
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

    let input_line = world.spawn((alloc(), TodoInputLine, ChildOf(root))).id();
    world.spawn((alloc(), TodoInputField, ChildOf(input_line)));
    world.spawn((alloc(), AddTodoButton, ChildOf(input_line)));

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
        root,
        list_container,
        footer_bar,
        next_node_id,
    });

    spawn_todo_item(world, "Buy milk".to_string(), false);
    spawn_todo_item(world, "Buy eggs".to_string(), true);
    spawn_todo_item(world, "Buy bread".to_string(), false);

    apply_filter_visibility(world);
    update_footer_visibility(world);
}

fn apply_filter_visibility(world: &mut World) {
    let active_filter = world.resource::<ActiveFilter>().0;
    let list_container = world.resource::<TodoRuntime>().list_container;

    let todo_rows: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<TodoItemRow>>();
        query.iter(world).collect()
    };

    for row in todo_rows {
        let completed = world.get::<Completed>(row).is_some_and(|state| state.0);
        let should_show = match active_filter {
            FilterType::All => true,
            FilterType::Active => !completed,
            FilterType::Completed => completed,
        };

        let is_visible = world
            .get::<ChildOf>(row)
            .is_some_and(|parent| parent.parent() == list_container);

        match (should_show, is_visible) {
            (true, false) => {
                world.entity_mut(row).insert(ChildOf(list_container));
            }
            (false, true) => {
                world.entity_mut(row).remove::<ChildOf>();
            }
            _ => {}
        }
    }
}

fn update_footer_visibility(world: &mut World) {
    let (root, footer_bar) = {
        let runtime = world.resource::<TodoRuntime>();
        (runtime.root, runtime.footer_bar)
    };

    let has_tasks = {
        let mut query = world.query_filtered::<Entity, With<TodoItemRow>>();
        query.iter(world).next().is_some()
    };

    let is_visible = world
        .get::<ChildOf>(footer_bar)
        .is_some_and(|parent| parent.parent() == root);

    match (has_tasks, is_visible) {
        (true, false) => {
            world.entity_mut(footer_bar).insert(ChildOf(root));
        }
        (false, true) => {
            world.entity_mut(footer_bar).remove::<ChildOf>();
        }
        _ => {}
    }
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
                if let Some(mut completed) = world.get_mut::<Completed>(entity) {
                    completed.0 = done;
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

    apply_filter_visibility(world);
    update_footer_visibility(world);
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
