use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use bevy_app::{App, PostUpdate, PreUpdate};
use bevy_ecs::{hierarchy::ChildOf, prelude::*};
use bevy_xilem::{
    ProjectionCtx, SynthesizedUiViews, UiNodeId, UiProjectorRegistry, UiRoot, UiSynthesisStats,
    UiView, gather_ui_roots, register_builtin_projectors, synthesize_roots_with_stats,
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use xilem_masonry::view::{FlexExt as _, flex_col, label, text_button};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FilterType {
    All,
    Active,
    Completed,
}

impl FilterType {
    fn as_str(self) -> &'static str {
        match self {
            FilterType::All => "All",
            FilterType::Active => "Active",
            FilterType::Completed => "Completed",
        }
    }
}

#[derive(Debug, Clone)]
enum TodoEvent {
    Add(String),
    Toggle(Entity),
    Delete(Entity),
    SetFilter(FilterType),
}

#[derive(Resource, Debug, Clone)]
struct TodoEventSender(Sender<TodoEvent>);

#[derive(Resource, Debug)]
struct TodoEventReceiver(Receiver<TodoEvent>);

#[derive(Component, Debug, Clone)]
struct TodoText(String);

#[derive(Component, Debug, Clone, Copy)]
struct Completed(bool);

#[derive(Component, Debug, Clone, Copy)]
struct TodoListContainer;

#[derive(Component, Debug, Clone, Copy)]
struct FilterTab(FilterType);

#[derive(Component, Debug, Clone, Copy)]
struct TodoItemRow;

#[derive(Component, Debug, Clone)]
struct AddTodoButton {
    template: String,
}

#[derive(Component, Debug, Clone, Copy)]
struct ToggleTodoButton {
    target: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
struct DeleteTodoButton {
    target: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
struct TodoItemLabel {
    target: Entity,
}

#[derive(Resource, Debug, Clone, Copy)]
struct ActiveFilter(FilterType);

#[derive(Resource, Debug, Clone, Copy)]
struct TodoRuntime {
    list_container: Entity,
    next_node_id: u64,
}

#[derive(Resource, Debug, Default)]
struct AddButtonIndex(HashMap<String, Entity>);

#[derive(Resource, Debug, Default)]
struct FilterButtonIndex(HashMap<FilterType, Entity>);

type ClickHandler = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Resource, Default)]
struct TodoCallbacks(Mutex<HashMap<Entity, ClickHandler>>);

fn alloc_node_id(world: &mut World) -> UiNodeId {
    let mut runtime = world.resource_mut::<TodoRuntime>();
    let id = UiNodeId(runtime.next_node_id);
    runtime.next_node_id += 1;
    id
}

fn store_callback(ctx: &ProjectionCtx<'_>, callback: ClickHandler) {
    if let Some(callbacks) = ctx.world.get_resource::<TodoCallbacks>()
        && let Ok(mut callback_map) = callbacks.0.lock()
    {
        callback_map.insert(ctx.entity, callback);
    }
}

fn project_todo_list_container(_: &TodoListContainer, ctx: ProjectionCtx<'_>) -> UiView {
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(flex_col(children))
}

fn project_todo_item_row(_: &TodoItemRow, ctx: ProjectionCtx<'_>) -> UiView {
    // This represents a single horizontal row conceptually (toggle + label + delete).
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(flex_col(children))
}

fn project_add_button(button: &AddTodoButton, ctx: ProjectionCtx<'_>) -> UiView {
    let sender = ctx.world.resource::<TodoEventSender>().0.clone();
    let template = button.template.clone();
    let callback: ClickHandler = Arc::new(move || {
        let _ = sender.send(TodoEvent::Add(template.clone()));
    });
    store_callback(&ctx, callback);

    let sender = ctx.world.resource::<TodoEventSender>().0.clone();
    let template = button.template.clone();

    Arc::new(text_button(
        format!("Add: {}", button.template),
        move |_| {
            let _ = sender.send(TodoEvent::Add(template.clone()));
        },
    ))
}

fn project_filter_tab(tab: &FilterTab, ctx: ProjectionCtx<'_>) -> UiView {
    let sender = ctx.world.resource::<TodoEventSender>().0.clone();
    let filter = tab.0;
    let callback: ClickHandler = Arc::new(move || {
        let _ = sender.send(TodoEvent::SetFilter(filter));
    });
    store_callback(&ctx, callback);

    let active = ctx.world.resource::<ActiveFilter>().0;
    let marker = if active == filter { "●" } else { "○" };

    let sender = ctx.world.resource::<TodoEventSender>().0.clone();
    Arc::new(text_button(
        format!("{marker} {}", filter.as_str()),
        move |_| {
            let _ = sender.send(TodoEvent::SetFilter(filter));
        },
    ))
}

fn project_toggle_button(toggle: &ToggleTodoButton, ctx: ProjectionCtx<'_>) -> UiView {
    let sender = ctx.world.resource::<TodoEventSender>().0.clone();
    let target = toggle.target;
    let callback: ClickHandler = Arc::new(move || {
        let _ = sender.send(TodoEvent::Toggle(target));
    });
    store_callback(&ctx, callback);

    let is_completed = ctx
        .world
        .get::<Completed>(target)
        .is_some_and(|completed| completed.0);

    let sender = ctx.world.resource::<TodoEventSender>().0.clone();
    Arc::new(text_button(
        if is_completed {
            "Undo".to_string()
        } else {
            "Done".to_string()
        },
        move |_| {
            let _ = sender.send(TodoEvent::Toggle(target));
        },
    ))
}

fn project_delete_button(delete: &DeleteTodoButton, ctx: ProjectionCtx<'_>) -> UiView {
    let sender = ctx.world.resource::<TodoEventSender>().0.clone();
    let target = delete.target;
    let callback: ClickHandler = Arc::new(move || {
        let _ = sender.send(TodoEvent::Delete(target));
    });
    store_callback(&ctx, callback);

    let sender = ctx.world.resource::<TodoEventSender>().0.clone();
    Arc::new(text_button("Delete", move |_| {
        let _ = sender.send(TodoEvent::Delete(target));
    }))
}

fn project_todo_item_label(label_component: &TodoItemLabel, ctx: ProjectionCtx<'_>) -> UiView {
    let text = ctx
        .world
        .get::<TodoText>(label_component.target)
        .map_or_else(
            || "<missing todo>".to_string(),
            |todo_text| todo_text.0.clone(),
        );

    let completed = ctx
        .world
        .get::<Completed>(label_component.target)
        .is_some_and(|status| status.0);

    Arc::new(label(if completed {
        format!("✔ {text}")
    } else {
        format!("○ {text}")
    }))
}

fn install_projectors(world: &mut World) {
    let mut registry = world.resource_mut::<UiProjectorRegistry>();
    register_builtin_projectors(&mut registry);
    registry
        .register_component::<TodoListContainer>(project_todo_list_container)
        .register_component::<TodoItemRow>(project_todo_item_row)
        .register_component::<AddTodoButton>(project_add_button)
        .register_component::<FilterTab>(project_filter_tab)
        .register_component::<ToggleTodoButton>(project_toggle_button)
        .register_component::<DeleteTodoButton>(project_delete_button)
        .register_component::<TodoItemLabel>(project_todo_item_label);
}

fn spawn_todo_item(world: &mut World, text: String) -> Entity {
    let list_container = world.resource::<TodoRuntime>().list_container;
    let row_node_id = alloc_node_id(world);
    let toggle_node_id = alloc_node_id(world);
    let label_node_id = alloc_node_id(world);
    let delete_node_id = alloc_node_id(world);

    let row = world
        .spawn((
            row_node_id,
            TodoItemRow,
            TodoText(text),
            Completed(false),
            ChildOf(list_container),
        ))
        .id();

    world.spawn((
        toggle_node_id,
        ToggleTodoButton { target: row },
        ChildOf(row),
    ));
    world.spawn((label_node_id, TodoItemLabel { target: row }, ChildOf(row)));
    world.spawn((
        delete_node_id,
        DeleteTodoButton { target: row },
        ChildOf(row),
    ));

    row
}

fn setup_todo_world(world: &mut World) {
    // ChildOf is the canonical Bevy parent-link component in this Bevy version.
    let mut next_node_id = 1_u64;
    let mut alloc = || {
        let id = UiNodeId(next_node_id);
        next_node_id += 1;
        id
    };

    let root = world
        .spawn((UiRoot, alloc(), bevy_xilem::UiFlexColumn))
        .id();

    let controls = world
        .spawn((alloc(), bevy_xilem::UiFlexColumn, ChildOf(root)))
        .id();

    let filter_bar = world
        .spawn((alloc(), bevy_xilem::UiFlexColumn, ChildOf(root)))
        .id();

    let list_container = world
        .spawn((alloc(), TodoListContainer, ChildOf(root)))
        .id();

    world.insert_resource(TodoRuntime {
        list_container,
        next_node_id,
    });

    let add_templates = ["Buy milk", "Write integration tests"];
    let mut add_index = HashMap::new();
    for template in add_templates {
        let node_id = alloc_node_id(world);
        let button = world
            .spawn((
                node_id,
                AddTodoButton {
                    template: template.to_string(),
                },
                ChildOf(controls),
            ))
            .id();
        add_index.insert(template.to_string(), button);
    }
    world.insert_resource(AddButtonIndex(add_index));

    let filter_tabs = [FilterType::All, FilterType::Active, FilterType::Completed];
    let mut filter_index = HashMap::new();
    for filter in filter_tabs {
        let node_id = alloc_node_id(world);
        let entity = world
            .spawn((node_id, FilterTab(filter), ChildOf(filter_bar)))
            .id();
        filter_index.insert(filter, entity);
    }
    world.insert_resource(FilterButtonIndex(filter_index));

    spawn_todo_item(world, "Read DESIGN.md".to_string());
    spawn_todo_item(world, "Review projector output".to_string());
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

fn drain_todo_events_and_mutate_world(world: &mut World) {
    let events: Vec<TodoEvent> = world.resource::<TodoEventReceiver>().0.try_iter().collect();
    if events.is_empty() {
        return;
    }

    for event in events {
        match event {
            TodoEvent::Add(text) => {
                if !text.trim().is_empty() {
                    spawn_todo_item(world, text);
                }
            }
            TodoEvent::Toggle(entity) => {
                if let Some(mut completed) = world.get_mut::<Completed>(entity) {
                    completed.0 = !completed.0;
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
}

fn synthesize_ui_and_build_callbacks(world: &mut World) {
    if let Some(callbacks) = world.get_resource::<TodoCallbacks>()
        && let Ok(mut callback_map) = callbacks.0.lock()
    {
        callback_map.clear();
    }

    let roots = gather_ui_roots(world);
    let (synthesized, stats) = world.resource_scope(|world, registry: Mut<UiProjectorRegistry>| {
        synthesize_roots_with_stats(world, &registry, roots)
    });

    world.resource_mut::<SynthesizedUiViews>().roots = synthesized;
    *world.resource_mut::<UiSynthesisStats>() = stats;
}

fn click_entity(app: &mut App, entity: Entity) {
    let callback = app
        .world()
        .resource::<TodoCallbacks>()
        .0
        .lock()
        .ok()
        .and_then(|map| map.get(&entity).cloned());

    if let Some(click) = callback {
        click();
        app.update();
    } else {
        eprintln!("No callback registered for entity {entity:?}");
    }
}

fn click_add_button(app: &mut App, template: &str) {
    let entity = app
        .world()
        .resource::<AddButtonIndex>()
        .0
        .get(template)
        .copied();

    if let Some(button) = entity {
        click_entity(app, button);
    } else {
        eprintln!("No Add button configured for '{template}'");
    }
}

fn click_filter_tab(app: &mut App, filter: FilterType) {
    let entity = app
        .world()
        .resource::<FilterButtonIndex>()
        .0
        .get(&filter)
        .copied();

    if let Some(button) = entity {
        click_entity(app, button);
    } else {
        eprintln!("No filter button for {}", filter.as_str());
    }
}

fn find_todo_by_text(app: &mut App, text: &str) -> Option<Entity> {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &TodoText)>();
    query.iter(world).find_map(
        |(entity, todo)| {
            if todo.0 == text { Some(entity) } else { None }
        },
    )
}

fn find_toggle_button_for(app: &mut App, target: Entity) -> Option<Entity> {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &ToggleTodoButton)>();
    query.iter(world).find_map(|(entity, toggle)| {
        if toggle.target == target {
            Some(entity)
        } else {
            None
        }
    })
}

fn find_delete_button_for(app: &mut App, target: Entity) -> Option<Entity> {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &DeleteTodoButton)>();
    query.iter(world).find_map(|(entity, delete)| {
        if delete.target == target {
            Some(entity)
        } else {
            None
        }
    })
}

fn toggle_todo_by_text(app: &mut App, text: &str) {
    if let Some(todo) = find_todo_by_text(app, text)
        && let Some(toggle_button) = find_toggle_button_for(app, todo)
    {
        click_entity(app, toggle_button);
    }
}

fn delete_todo_by_text(app: &mut App, text: &str) {
    if let Some(todo) = find_todo_by_text(app, text)
        && let Some(delete_button) = find_delete_button_for(app, todo)
    {
        click_entity(app, delete_button);
    }
}

fn print_todo_snapshot(app: &mut App) {
    let (active_filter, list_container) = {
        let world = app.world();
        (
            world.resource::<ActiveFilter>().0,
            world.resource::<TodoRuntime>().list_container,
        )
    };

    let mut rows = {
        let world = app.world_mut();
        let mut query = world.query::<(Entity, &TodoText, &Completed, Option<&ChildOf>)>();

        let mut collected = Vec::new();
        for (entity, text, completed, parent) in query.iter(world) {
            let visible = parent.is_some_and(|p| p.parent() == list_container);
            collected.push((entity, text.0.clone(), completed.0, visible));
        }

        collected
    };

    rows.sort_by(|a, b| a.1.cmp(&b.1));

    let stats = app.world().resource::<UiSynthesisStats>();

    println!(
        "\nfilter = {} | roots={} nodes={} unhandled={}",
        active_filter.as_str(),
        stats.root_count,
        stats.node_count,
        stats.unhandled_count
    );

    for (entity, text, completed, visible) in rows {
        let done = if completed { "x" } else { " " };
        let shown = if visible { "visible" } else { "hidden" };
        println!("[{done}] {text:<26} ({shown}, {entity:?})");
    }
}

fn main() {
    let mut app = App::new();
    let (sender, receiver) = unbounded::<TodoEvent>();

    app.init_resource::<UiProjectorRegistry>()
        .init_resource::<SynthesizedUiViews>()
        .init_resource::<UiSynthesisStats>()
        .init_resource::<TodoCallbacks>()
        .init_resource::<AddButtonIndex>()
        .init_resource::<FilterButtonIndex>()
        .insert_resource(ActiveFilter(FilterType::All))
        .insert_resource(TodoEventSender(sender))
        .insert_resource(TodoEventReceiver(receiver));

    install_projectors(app.world_mut());
    setup_todo_world(app.world_mut());

    app.add_systems(PreUpdate, drain_todo_events_and_mutate_world)
        .add_systems(PostUpdate, synthesize_ui_and_build_callbacks);

    app.update();

    println!("\n== Todo List demo (ECS entities + MPSC + synthesized UI) ==");
    print_todo_snapshot(&mut app);

    click_add_button(&mut app, "Buy milk");
    print_todo_snapshot(&mut app);

    click_add_button(&mut app, "Write integration tests");
    print_todo_snapshot(&mut app);

    toggle_todo_by_text(&mut app, "Buy milk");
    print_todo_snapshot(&mut app);

    click_filter_tab(&mut app, FilterType::Active);
    print_todo_snapshot(&mut app);

    click_filter_tab(&mut app, FilterType::Completed);
    print_todo_snapshot(&mut app);

    delete_todo_by_text(&mut app, "Buy milk");
    print_todo_snapshot(&mut app);

    click_filter_tab(&mut app, FilterType::All);
    print_todo_snapshot(&mut app);
}
