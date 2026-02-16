use std::sync::Arc;

use bevy_app::{App, PostUpdate, PreUpdate};
use bevy_ecs::{hierarchy::ChildOf, prelude::*};
use bevy_xilem::{
    BevyXilemRuntime, ProjectionCtx, SynthesizedUiViews, UiNodeId, UiProjectorRegistry, UiRoot,
    UiSynthesisStats, UiView, gather_ui_roots, register_builtin_projectors,
    synthesize_roots_with_stats,
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use xilem::{
    EventLoop, WidgetView, WindowOptions, Xilem,
    core::{Edit, map_state},
    winit::{dpi::LogicalSize, error::EventLoopError},
};
use xilem_masonry::view::{label, text_button};

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

fn alloc_node_id(world: &mut World) -> UiNodeId {
    let mut runtime = world.resource_mut::<TodoRuntime>();
    let id = UiNodeId(runtime.next_node_id);
    runtime.next_node_id += 1;
    id
}

fn project_add_button(button: &AddTodoButton, ctx: ProjectionCtx<'_>) -> UiView {
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
    let active = ctx.world.resource::<ActiveFilter>().0;
    let filter = tab.0;
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
    let target = toggle.target;
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
    let target = delete.target;
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
            bevy_xilem::UiFlexRow,
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
        .spawn((alloc(), bevy_xilem::UiFlexRow, ChildOf(root)))
        .id();

    let filter_bar = world
        .spawn((alloc(), bevy_xilem::UiFlexRow, ChildOf(root)))
        .id();

    let list_container = world
        .spawn((
            alloc(),
            bevy_xilem::UiFlexColumn,
            TodoListContainer,
            ChildOf(root),
        ))
        .id();

    world.insert_resource(TodoRuntime {
        list_container,
        next_node_id,
    });

    let add_templates = ["Buy milk", "Write integration tests"];
    for template in add_templates {
        let node_id = alloc_node_id(world);
        world.spawn((
            node_id,
            AddTodoButton {
                template: template.to_string(),
            },
            ChildOf(controls),
        ));
    }

    let filter_tabs = [FilterType::All, FilterType::Active, FilterType::Completed];
    for filter in filter_tabs {
        let node_id = alloc_node_id(world);
        world.spawn((node_id, FilterTab(filter), ChildOf(filter_bar)));
    }

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

fn synthesize_ui(world: &mut World) {
    let roots = gather_ui_roots(world);
    let (synthesized, stats) = world.resource_scope(|world, registry: Mut<UiProjectorRegistry>| {
        synthesize_roots_with_stats(world, &registry, roots)
    });

    world.resource_mut::<SynthesizedUiViews>().roots = synthesized;
    *world.resource_mut::<UiSynthesisStats>() = stats;
}

fn build_bevy_todo_app() -> App {
    let mut app = App::new();
    let (sender, receiver) = unbounded::<TodoEvent>();

    app.init_resource::<UiProjectorRegistry>()
        .init_resource::<SynthesizedUiViews>()
        .init_resource::<UiSynthesisStats>()
        .insert_resource(ActiveFilter(FilterType::All))
        .insert_resource(TodoEventSender(sender))
        .insert_resource(TodoEventReceiver(receiver));

    install_projectors(app.world_mut());
    setup_todo_world(app.world_mut());

    app.add_systems(PreUpdate, drain_todo_events_and_mutate_world)
        .add_systems(PostUpdate, synthesize_ui);

    app.update();

    app
}

fn todo_app_logic(
    runtime: &mut BevyXilemRuntime,
) -> impl WidgetView<Edit<BevyXilemRuntime>> + use<> {
    runtime.update();

    let root_view = runtime.first_root_or_label("No synthesized todo root");

    map_state(root_view, |_runtime: &mut BevyXilemRuntime, _| ())
}

fn main() -> Result<(), EventLoopError> {
    let runtime = BevyXilemRuntime::new(build_bevy_todo_app());

    let app = Xilem::new_simple(
        runtime,
        todo_app_logic,
        WindowOptions::new("Bevy Xilem Todo List")
            .with_initial_inner_size(LogicalSize::new(680.0, 720.0)),
    );

    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
