use std::sync::Arc;

use crate::{
    AppBevyXilemExt, BevyXilemPlugin, ColorStyle, Hovered, Pressed, ProjectionCtx, Selector,
    StyleRule, StyleSetter, StyleSheet, UiEventQueue, UiProjectorRegistry, UiRoot, UiView,
    ecs_button, register_builtin_projectors, resolve_style, resolve_style_for_entity_classes,
    synthesize_roots_with_stats,
};
use bevy_app::App;
use bevy_ecs::{hierarchy::ChildOf, prelude::*};

#[derive(Component, Debug, Clone, Copy)]
struct TestRoot;

#[derive(Component, Debug, Clone, Copy)]
struct TypeStyled;

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
    app.add_plugins(BevyXilemPlugin)
        .register_projector::<TestRoot>(project_test_root);

    app.world_mut().spawn((UiRoot, TestRoot));

    app.update();

    let synthesized = app.world().resource::<crate::SynthesizedUiViews>();
    assert_eq!(synthesized.roots.len(), 1);

    let _runtime = app.world().non_send_resource::<crate::MasonryRuntime>();
}

#[test]
fn ui_event_queue_drains_typed_actions() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .register_projector::<TestRoot>(project_test_root);

    let root = app.world_mut().spawn((UiRoot, TestRoot)).id();

    // Build synthesized tree + initial Masonry retained tree.
    app.update();

    app.world()
        .resource::<UiEventQueue>()
        .push_typed(root, TestAction::Clicked);

    let actions = app
        .world_mut()
        .resource_mut::<UiEventQueue>()
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

    let root = world.spawn((UiRoot, crate::UiLabel::new("ok"))).id();

    let (roots, stats) = synthesize_roots_with_stats(&world, &registry, [root]);

    assert_eq!(roots.len(), 1);
    assert_eq!(stats.unhandled_count, 0);
    assert_eq!(stats.missing_entity_count, 0);
}

#[test]
fn resolve_style_for_entity_classes_applies_hover_pseudo_state() {
    let mut world = World::new();
    let mut sheet = StyleSheet::default();
    let base = crate::xilem::Color::from_rgb8(0x11, 0x22, 0x33);
    let hover = crate::xilem::Color::from_rgb8(0xAA, 0xBB, 0xCC);

    sheet.set_class(
        "test.button",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(base),
                hover_bg: Some(hover),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );
    world.insert_resource(sheet);

    let entity = world.spawn((Hovered,)).id();
    let resolved = resolve_style_for_entity_classes(&world, entity, ["test.button"]);

    assert_eq!(resolved.colors.bg, Some(hover));
}

#[test]
fn selector_and_rule_applies_hover_and_pressed_states() {
    let mut world = World::new();
    let mut sheet = StyleSheet::default();

    let base = crate::xilem::Color::from_rgb8(0x22, 0x22, 0x22);
    let hover = crate::xilem::Color::from_rgb8(0x44, 0x44, 0x44);
    let pressed = crate::xilem::Color::from_rgb8(0x66, 0x66, 0x66);

    sheet.add_rule(StyleRule::new(
        Selector::class("test.button"),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(base),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));
    sheet.add_rule(StyleRule::new(
        Selector::and(vec![
            Selector::class("test.button"),
            Selector::pseudo(crate::PseudoClass::Hovered),
        ]),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(hover),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));
    sheet.add_rule(StyleRule::new(
        Selector::and(vec![
            Selector::class("test.button"),
            Selector::pseudo(crate::PseudoClass::Pressed),
        ]),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(pressed),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    world.insert_resource(sheet);

    let entity = world
        .spawn((
            crate::StyleClass(vec!["test.button".to_string()]),
            Hovered,
            Pressed,
        ))
        .id();

    crate::mark_style_dirty(&mut world);
    crate::sync_style_targets(&mut world);

    let resolved = resolve_style(&world, entity);
    assert_eq!(resolved.colors.bg, Some(pressed));
}

#[test]
fn selector_type_rule_matches_component_type() {
    let mut world = World::new();
    let mut sheet = StyleSheet::default();
    let type_color = crate::xilem::Color::from_rgb8(0x12, 0x34, 0x56);

    sheet.add_rule(StyleRule::new(
        Selector::of_type::<TypeStyled>(),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(type_color),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));
    world.insert_resource(sheet);

    let entity = world.spawn((TypeStyled,)).id();
    crate::mark_style_dirty(&mut world);
    crate::sync_style_targets(&mut world);

    let resolved = resolve_style(&world, entity);
    assert_eq!(resolved.colors.bg, Some(type_color));
}

#[test]
fn selector_descendant_rule_matches_nested_entity_and_updates_on_ancestor_change() {
    let mut world = World::new();
    let mut sheet = StyleSheet::default();

    let dark_bg = crate::xilem::Color::from_rgb8(0x20, 0x2A, 0x44);
    let light_bg = crate::xilem::Color::from_rgb8(0xE8, 0xEE, 0xFF);

    sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.dark"),
            Selector::class("gallery.target"),
        ),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(dark_bg),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.light"),
            Selector::class("gallery.target"),
        ),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(light_bg),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    world.insert_resource(sheet);

    let root = world
        .spawn((crate::StyleClass(vec!["theme.dark".to_string()]),))
        .id();
    let child = world
        .spawn((
            crate::StyleClass(vec!["gallery.target".to_string()]),
            ChildOf(root),
        ))
        .id();

    crate::mark_style_dirty(&mut world);
    crate::sync_style_targets(&mut world);
    assert_eq!(resolve_style(&world, child).colors.bg, Some(dark_bg));

    world.clear_trackers();
    world
        .entity_mut(root)
        .insert(crate::StyleClass(vec!["theme.light".to_string()]));

    crate::mark_style_dirty(&mut world);
    crate::sync_style_targets(&mut world);
    assert_eq!(resolve_style(&world, child).colors.bg, Some(light_bg));
}

#[test]
fn sync_style_targets_restarts_tween_when_current_differs_but_target_unchanged() {
    let mut world = World::new();
    let mut sheet = StyleSheet::default();

    let base = crate::xilem::Color::from_rgb8(0x20, 0x2A, 0x44);
    let mid = crate::xilem::Color::from_rgb8(0x90, 0x99, 0xB3);

    sheet.set_class(
        "test.animated",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(base),
                ..ColorStyle::default()
            },
            transition: Some(crate::StyleTransition { duration: 0.2 }),
            ..StyleSetter::default()
        },
    );

    world.insert_resource(sheet);

    let entity = world
        .spawn((crate::StyleClass(vec!["test.animated".to_string()]),))
        .id();

    crate::mark_style_dirty(&mut world);
    crate::sync_style_targets(&mut world);

    world.entity_mut(entity).insert(crate::CurrentColorStyle {
        bg: Some(mid),
        text: None,
        border: None,
    });
    world.entity_mut(entity).insert(crate::TargetColorStyle {
        bg: Some(base),
        text: None,
        border: None,
    });
    world.entity_mut(entity).insert(crate::StyleDirty);

    crate::sync_style_targets(&mut world);

    assert_eq!(
        world
            .get::<crate::TargetColorStyle>(entity)
            .and_then(|target| target.bg),
        Some(base)
    );
    assert!(world.get::<bevy_tweening::TweenAnim>(entity).is_some());
}

#[test]
fn pointer_left_does_not_clear_pressed_marker() {
    let mut world = World::new();
    world.insert_resource(UiEventQueue::default());

    let entity = world.spawn((crate::Hovered, crate::Pressed)).id();

    world
        .resource::<UiEventQueue>()
        .push_typed(entity, crate::UiInteractionEvent::PointerLeft);

    crate::sync_ui_interaction_markers(&mut world);

    assert!(world.get::<crate::Hovered>(entity).is_none());
    assert!(world.get::<crate::Pressed>(entity).is_some());
}
