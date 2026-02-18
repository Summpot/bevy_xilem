mod utils;

use std::sync::Arc;

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ColorStyle, LayoutStyle, ProjectionCtx, StyleClass,
    StyleSetter, StyleSheet, StyleTransition, TextStyle, UiEventQueue, UiRoot, UiView,
    apply_label_style, apply_text_input_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::{
        hierarchy::{ChildOf, Children},
        prelude::*,
    },
    button, checkbox, emit_ui_action, resolve_style, resolve_style_for_classes,
    resolve_style_for_entity_classes, run_app, text_input,
    xilem::{
        Color, InsertNewline,
        masonry::{layout::Length, theme::DEFAULT_GAP},
        view::{
            FlexExt as _, FlexSpacer, MainAxisAlignment, flex_col, flex_row, label, sized_box,
            virtual_scroll,
        },
        winit::error::EventLoopError,
    },
};
use utils::init_logging;

const LIST_VIEWPORT_HEIGHT: f64 = 360.0;

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
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(flex_col(children), &style))
}

fn project_todo_header(_: &TodoHeader, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    Arc::new(apply_label_style(label("todos"), &style))
}

fn project_todo_input_area(_: &TodoInputArea, ctx: ProjectionCtx<'_>) -> UiView {
    let area_style = resolve_style(ctx.world, ctx.entity);
    let input_style = resolve_style_for_classes(ctx.world, ["todo.input"]);
    let add_button_style =
        resolve_style_for_entity_classes(ctx.world, ctx.entity, ["todo.add-button"]);

    let draft = ctx.world.resource::<DraftTodo>().0.clone();
    let entity_for_enter = ctx.entity;

    Arc::new(apply_widget_style(
        flex_row((
            apply_text_input_style(
                text_input(ctx.entity, draft, TodoEvent::SetDraft)
                    .placeholder("What needs to be done?")
                    .insert_newline(InsertNewline::OnShiftEnter)
                    .on_enter(move |_, _| {
                        emit_ui_action(entity_for_enter, TodoEvent::SubmitDraft);
                    }),
                &input_style,
            )
            .flex(1.0),
            apply_widget_style(
                button(ctx.entity, TodoEvent::SubmitDraft, "Add task"),
                &add_button_style,
            ),
        )),
        &area_style,
    ))
}

fn project_todo_list_container(_: &TodoListContainer, ctx: ProjectionCtx<'_>) -> UiView {
    let container_style = resolve_style(ctx.world, ctx.entity);
    let empty_style = resolve_style_for_classes(ctx.world, ["todo.empty"]);
    let viewport_style = resolve_style_for_classes(ctx.world, ["todo.list-viewport"]);

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
        return Arc::new(apply_widget_style(
            apply_label_style(label("No tasks for this filter."), &empty_style),
            &container_style,
        ));
    }

    let visible_children = Arc::new(visible_children);
    let item_count = i64::try_from(visible_children.len()).unwrap_or(i64::MAX);

    Arc::new(apply_widget_style(
        apply_widget_style(
            sized_box(virtual_scroll(0..item_count, {
                let visible_children = Arc::clone(&visible_children);
                move |_, idx| {
                    let index =
                        usize::try_from(idx).expect("virtual scroll index should be positive");
                    visible_children
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| Arc::new(label("")))
                }
            }))
            .fixed_height(Length::px(LIST_VIEWPORT_HEIGHT)),
            &viewport_style,
        ),
        &container_style,
    ))
}

fn project_todo_item(item: &TodoItem, ctx: ProjectionCtx<'_>) -> UiView {
    let entity = ctx.entity;
    let style = resolve_style(ctx.world, ctx.entity);
    let checkbox_style = resolve_style_for_classes(ctx.world, ["todo.item-checkbox"]);
    let delete_button_style =
        resolve_style_for_entity_classes(ctx.world, entity, ["todo.delete-button"]);

    Arc::new(apply_widget_style(
        flex_row((
            apply_widget_style(
                checkbox(entity, item.text.clone(), item.completed, move |value| {
                    TodoEvent::SetCompleted(entity, value)
                })
                .text_size(checkbox_style.text.size),
                &checkbox_style,
            ),
            FlexSpacer::Flex(1.0),
            apply_widget_style(
                button(entity, TodoEvent::Delete(entity), "Delete"),
                &delete_button_style,
            ),
        )),
        &style,
    ))
}

fn project_filter_bar(_: &TodoFilterBar, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
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

    Arc::new(apply_widget_style(
        flex_row(children).main_axis_alignment(MainAxisAlignment::Center),
        &style,
    ))
}

fn project_filter_toggle(filter_toggle: &FilterToggle, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let filter = filter_toggle.0;
    let active = ctx.world.resource::<ActiveFilter>().0;

    Arc::new(apply_widget_style(
        checkbox(ctx.entity, filter.as_str(), active == filter, move |_| {
            TodoEvent::SetFilter(filter)
        })
        .text_size(style.text.size),
        &style,
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
            StyleClass(vec!["todo.item".to_string()]),
            TodoItem {
                text,
                completed: done,
            },
            ChildOf(list_container),
        ))
        .id()
}

fn setup_todo_world(mut commands: Commands) {
    let root = commands
        .spawn((
            UiRoot,
            TodoRootView,
            StyleClass(vec!["todo.root".to_string()]),
        ))
        .id();

    commands.spawn((
        TodoHeader,
        StyleClass(vec!["todo.header".to_string()]),
        ChildOf(root),
    ));
    commands.spawn((
        TodoInputArea,
        StyleClass(vec!["todo.input-area".to_string()]),
        ChildOf(root),
    ));

    let list_container = commands
        .spawn((
            TodoListContainer,
            StyleClass(vec!["todo.list-container".to_string()]),
            ChildOf(root),
        ))
        .id();

    let footer_bar = commands
        .spawn((
            TodoFilterBar,
            StyleClass(vec!["todo.filter-bar".to_string()]),
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        FilterToggle(FilterType::All),
        StyleClass(vec!["todo.filter-toggle".to_string()]),
        ChildOf(footer_bar),
    ));
    commands.spawn((
        FilterToggle(FilterType::Active),
        StyleClass(vec!["todo.filter-toggle".to_string()]),
        ChildOf(footer_bar),
    ));
    commands.spawn((
        FilterToggle(FilterType::Completed),
        StyleClass(vec!["todo.filter-toggle".to_string()]),
        ChildOf(footer_bar),
    ));

    commands.insert_resource(TodoRuntime { list_container });

    for i in 1..=120 {
        commands.spawn((
            StyleClass(vec!["todo.item".to_string()]),
            TodoItem {
                text: format!("Sample task #{i}"),
                completed: i % 3 == 0,
            },
            ChildOf(list_container),
        ));
    }
}

fn setup_todo_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "todo.root",
        StyleSetter {
            layout: LayoutStyle {
                gap: Some(4.0),
                padding: Some(50.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.header",
        StyleSetter {
            text: TextStyle { size: Some(80.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xE5, 0xE7, 0xEB)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.input-area",
        StyleSetter {
            layout: LayoutStyle {
                gap: Some(DEFAULT_GAP.get()),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.input",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            layout: LayoutStyle {
                padding: Some(6.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x22, 0x22, 0x22)),
                border: Some(Color::from_rgb8(0x3F, 0x3F, 0x46)),
                text: Some(Color::from_rgb8(0xF4, 0xF4, 0xF5)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.add-button",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(6.0),
                corner_radius: Some(8.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x25, 0x63, 0xEB)),
                hover_bg: Some(Color::from_rgb8(0x1D, 0x4E, 0xD8)),
                pressed_bg: Some(Color::from_rgb8(0x1E, 0x40, 0xAF)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.12 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.add-label",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class("todo.list-container", StyleSetter::default());

    style_sheet.set_class(
        "todo.empty",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                border: Some(Color::from_rgb8(0x27, 0x2A, 0x36)),
                text: Some(Color::from_rgb8(0xA1, 0xA1, 0xAA)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.list-viewport",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(4.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                border: Some(Color::from_rgb8(0x27, 0x2A, 0x36)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.item",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(DEFAULT_GAP.get()),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x16, 0x17, 0x1C)),
                hover_bg: Some(Color::from_rgb8(0x20, 0x22, 0x2B)),
                border: Some(Color::from_rgb8(0x27, 0x2A, 0x36)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.15 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.item-checkbox",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xE4, 0xE4, 0xE7)),
                hover_text: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.15 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.delete-button",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(5.0),
                corner_radius: Some(6.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x45, 0x45, 0x45)),
                hover_bg: Some(Color::from_rgb8(0x55, 0x55, 0x55)),
                pressed_bg: Some(Color::from_rgb8(0x35, 0x35, 0x35)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.12 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.delete-label",
        StyleSetter {
            text: TextStyle { size: Some(14.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xFA, 0xFA, 0xFA)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.filter-bar",
        StyleSetter {
            layout: LayoutStyle {
                gap: Some(DEFAULT_GAP.get()),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "todo.filter-toggle",
        StyleSetter {
            text: TextStyle { size: Some(14.0) },
            layout: LayoutStyle {
                padding: Some(4.0),
                corner_radius: Some(6.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x22, 0x23, 0x28)),
                hover_bg: Some(Color::from_rgb8(0x2D, 0x30, 0x3A)),
                pressed_bg: Some(Color::from_rgb8(0x1B, 0x1D, 0x24)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.15 }),
            ..StyleSetter::default()
        },
    );
}

fn drain_todo_events_and_mutate_world(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
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
    init_logging();

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
        .add_systems(Startup, (setup_todo_styles, setup_todo_world));

    app.add_systems(PreUpdate, drain_todo_events_and_mutate_world);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app(build_bevy_todo_app(), "To Do MVC")
}
