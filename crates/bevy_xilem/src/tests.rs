use std::sync::Arc;

use crate::{
    BevyXilemPlugin, ProjectionCtx, UiEventQueue, UiNodeId, UiProjectorRegistry, UiRoot, UiView,
    ecs_button, register_builtin_projectors, synthesize_roots_with_stats,
};
use bevy_app::App;
use bevy_ecs::prelude::*;

#[derive(Component, Debug, Clone, Copy)]
struct TestRoot;

#[derive(Debug, Clone, PartialEq, Eq)]
enum TestAction {
    Clicked,
}

fn project_test_root(_: &TestRoot, ctx: ProjectionCtx<'_>) -> UiView {
    Arc::new(ecs_button(ctx.entity, TestAction::Clicked, "Click"))
}

#[test]
fn plugin_wires_synthesis_and_runtime() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin);

    {
        let mut registry = app.world_mut().resource_mut::<UiProjectorRegistry>();
        registry.register_component::<TestRoot>(project_test_root);
    }

    app.world_mut().spawn((UiRoot, UiNodeId(1), TestRoot));

    app.update();

    let synthesized = app.world().resource::<crate::SynthesizedUiViews>();
    assert_eq!(synthesized.roots.len(), 1);

    let _runtime = app.world().non_send::<crate::MasonryRuntime>();
}

#[test]
fn ui_event_queue_drains_typed_actions() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin);

    {
        let mut registry = app.world_mut().resource_mut::<UiProjectorRegistry>();
        registry.register_component::<TestRoot>(project_test_root);
    }

    let root = app.world_mut().spawn((UiRoot, UiNodeId(1), TestRoot)).id();

    // Build synthesized tree + initial Masonry retained tree.
    app.update();

    app.world()
        .resource::<UiEventQueue>()
        .push_typed(root, TestAction::Clicked);

    let actions = app
        .world()
        .resource::<UiEventQueue>()
        .drain_actions::<TestAction>();

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].entity, root);
    assert_eq!(actions[0].action, TestAction::Clicked);
}

#[test]
fn synthesis_stats_track_missing_entity() {
    let mut world = World::new();
    let mut registry = UiProjectorRegistry::default();
    register_builtin_projectors(&mut registry);

    let stale_root = world.spawn_empty().id();
    assert!(world.despawn(stale_root));

    let (_roots, stats) = synthesize_roots_with_stats(&world, &registry, [stale_root]);

    assert_eq!(stats.root_count, 1);
    assert_eq!(stats.node_count, 1);
    assert_eq!(stats.missing_entity_count, 1);
    assert_eq!(stats.cycle_count, 0);
}

#[test]
fn builtin_registry_projects_label() {
    let mut world = World::new();
    let mut registry = UiProjectorRegistry::default();
    register_builtin_projectors(&mut registry);

    let root = world
        .spawn((UiRoot, UiNodeId(1), crate::UiLabel::new("ok")))
        .id();

    let (roots, stats) = synthesize_roots_with_stats(&world, &registry, [root]);

    assert_eq!(roots.len(), 1);
    assert_eq!(stats.unhandled_count, 0);
    assert!(roots[0].as_any().is::<xilem_masonry::view::Label>());
}
