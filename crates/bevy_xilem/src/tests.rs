use std::{
    sync::{Arc, Once},
    time::Duration,
};

use crate::{
    AppBevyXilemExt, AppI18n, BevyXilemPlugin, ColorStyle, Hovered, Pressed, ProjectionCtx,
    Selector, StyleRule, StyleSetter, StyleSheet, SyncTextSource, UiEventQueue,
    UiProjectorRegistry, UiRoot, UiView, bubble_ui_pointer_events, ecs_button,
    ensure_overlay_defaults, ensure_overlay_root, ensure_overlay_root_entity,
    handle_overlay_actions, register_builtin_projectors, reparent_overlay_entities, resolve_style,
    resolve_style_for_entity_classes, spawn_in_overlay_root, synthesize_roots_with_stats,
};
use bevy_app::App;
use bevy_ecs::{hierarchy::ChildOf, prelude::*};
use bevy_input::{
    ButtonInput, ButtonState,
    mouse::{MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel},
};
use bevy_math::{Rect, Vec2};
use bevy_tweening::Lens;
use bevy_window::{CursorMoved, PrimaryWindow, Window, WindowResized};
use masonry::core::{Widget, WidgetRef};

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

fn init_test_tracing() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new("bevy_xilem=debug"))
            .with_test_writer()
            .try_init();
    });
}

#[test]
fn plugin_wires_synthesis_and_runtime() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .register_projector::<TestRoot>(project_test_root);

    app.world_mut().spawn((UiRoot, TestRoot));

    app.update();

    let synthesized = app.world().resource::<crate::SynthesizedUiViews>();
    assert_eq!(synthesized.roots.len(), 2);

    let _runtime = app.world().non_send_resource::<crate::MasonryRuntime>();
}

#[test]
fn input_bridge_uses_primary_window_cursor_for_click_and_emits_move_before_down_up() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin);

    let mut window = Window::default();
    window.resolution.set(800.0, 600.0);
    window.set_cursor_position(Some(Vec2::new(320.0, 180.0)));
    let window_entity = app.world_mut().spawn((window, PrimaryWindow)).id();

    app.update();

    // CursorMoved payload is intentionally different from Window::cursor_position().
    // The bridge should trust Window state.
    app.world_mut().write_message(CursorMoved {
        window: window_entity,
        position: Vec2::new(12.0, 24.0),
        delta: None,
    });
    app.update();

    {
        let mut runtime = app
            .world_mut()
            .non_send_resource_mut::<crate::MasonryRuntime>();
        runtime.clear_pointer_trace_for_tests();
    }

    app.world_mut().write_message(MouseButtonInput {
        button: MouseButton::Left,
        state: ButtonState::Pressed,
        window: window_entity,
    });
    app.world_mut().write_message(MouseButtonInput {
        button: MouseButton::Left,
        state: ButtonState::Released,
        window: window_entity,
    });

    app.update();

    let runtime = app.world().non_send_resource::<crate::MasonryRuntime>();
    assert_eq!(
        runtime.pointer_position_for_tests(),
        Vec2::new(320.0, 180.0)
    );
    assert_eq!(
        runtime.pointer_trace_for_tests(),
        &[
            crate::runtime::PointerTraceEvent::Move,
            crate::runtime::PointerTraceEvent::Down,
            crate::runtime::PointerTraceEvent::Move,
            crate::runtime::PointerTraceEvent::Up,
        ]
    );
}

#[test]
fn input_bridge_uses_primary_window_cursor_for_mouse_wheel_events() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin);

    let mut window = Window::default();
    window.resolution.set(800.0, 600.0);
    window.set_cursor_position(Some(Vec2::new(144.0, 96.0)));
    let window_entity = app.world_mut().spawn((window, PrimaryWindow)).id();

    app.update();

    app.world_mut().write_message(CursorMoved {
        window: window_entity,
        position: Vec2::new(8.0, 8.0),
        delta: None,
    });
    app.update();

    {
        let mut runtime = app
            .world_mut()
            .non_send_resource_mut::<crate::MasonryRuntime>();
        runtime.clear_pointer_trace_for_tests();
    }

    app.world_mut().write_message(MouseWheel {
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: -1.0,
        window: window_entity,
    });

    app.update();

    let runtime = app.world().non_send_resource::<crate::MasonryRuntime>();
    assert_eq!(runtime.pointer_position_for_tests(), Vec2::new(144.0, 96.0));
    assert_eq!(
        runtime.pointer_trace_for_tests(),
        &[
            crate::runtime::PointerTraceEvent::Move,
            crate::runtime::PointerTraceEvent::Scroll,
        ]
    );
}

#[test]
fn input_bridge_uses_primary_window_logical_size_for_resize_events() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin);

    let mut window = Window::default();
    window.resolution.set(800.0, 600.0);
    let window_entity = app.world_mut().spawn((window, PrimaryWindow)).id();

    app.update();

    {
        let world = app.world_mut();
        let mut query = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut primary_window = query
            .single_mut(world)
            .expect("primary window should exist");
        primary_window.resolution.set(1280.0, 720.0);
    }

    // Event payload is intentionally stale/incorrect; bridge should trust Window state.
    app.world_mut().write_message(WindowResized {
        window: window_entity,
        width: 1.0,
        height: 1.0,
    });

    app.update();

    let runtime = app.world().non_send_resource::<crate::MasonryRuntime>();
    assert_eq!(runtime.viewport_size(), (1280.0, 720.0));
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
fn plugin_initializes_app_i18n_resource() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin);

    assert!(app.world().contains_resource::<AppI18n>());
}

#[test]
fn app_i18n_resolves_showcase_hello_world_for_zh_cn() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin).register_i18n_bundle(
        "zh-CN",
        SyncTextSource::String(include_str!("../../../assets/locales/zh-CN/main.ftl")),
        vec!["Inter", "Noto Sans CJK SC", "sans-serif"],
    );

    assert_eq!(
        app.world().resource::<AppI18n>().translate("hello_world"),
        "你好，世界！"
    );
}

#[test]
fn resolve_localized_text_prefers_translation_over_uilabel_fallback() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin).register_i18n_bundle(
        "zh-CN",
        SyncTextSource::String(include_str!("../../../assets/locales/zh-CN/main.ftl")),
        vec!["Inter", "Noto Sans CJK SC", "sans-serif"],
    );

    let entity = app
        .world_mut()
        .spawn((
            crate::UiLabel::new("Hello world"),
            crate::LocalizeText::new("hello_world"),
        ))
        .id();

    let resolved = crate::resolve_localized_text(app.world(), entity, "Hello world");

    assert_eq!(resolved, "你好，世界！");
}

#[test]
fn localized_text_updates_after_active_locale_change() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .insert_resource(AppI18n::new(
            "en-US"
                .parse()
                .expect("en-US locale identifier should parse"),
        ))
        .register_i18n_bundle(
            "en-US",
            SyncTextSource::String(include_str!("../../../assets/locales/en-US/main.ftl")),
            vec!["Inter", "sans-serif"],
        )
        .register_i18n_bundle(
            "zh-CN",
            SyncTextSource::String(include_str!("../../../assets/locales/zh-CN/main.ftl")),
            vec!["Inter", "Noto Sans CJK SC", "sans-serif"],
        );

    let entity = app
        .world_mut()
        .spawn((
            crate::UiLabel::new("Hello world"),
            crate::LocalizeText::new("hello_world"),
        ))
        .id();

    let resolved_en = crate::resolve_localized_text(app.world(), entity, "Hello world");

    assert_eq!(resolved_en, "Hello, world!");

    app.world_mut().resource_mut::<AppI18n>().set_active_locale(
        "zh-CN"
            .parse()
            .expect("zh-CN locale identifier should parse"),
    );

    let resolved_zh = crate::resolve_localized_text(app.world(), entity, "Hello world");

    assert_eq!(resolved_zh, "你好，世界！");
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

#[test]
fn sync_style_targets_keeps_unmanaged_tween_anim() {
    let mut world = World::new();

    let tween = bevy_tweening::Tween::new(
        bevy_tweening::EaseMethod::default(),
        Duration::from_secs(1),
        crate::ColorStyleLens {
            start: crate::CurrentColorStyle {
                bg: Some(crate::xilem::Color::from_rgb8(0x10, 0x20, 0x30)),
                text: None,
                border: None,
            },
            end: crate::CurrentColorStyle {
                bg: Some(crate::xilem::Color::from_rgb8(0x40, 0x50, 0x60)),
                text: None,
                border: None,
            },
        },
    );

    let entity = world.spawn((bevy_tweening::TweenAnim::new(tween),)).id();
    world.entity_mut(entity).insert(crate::StyleDirty);

    crate::sync_style_targets(&mut world);

    assert!(world.get::<bevy_tweening::TweenAnim>(entity).is_some());
}

#[test]
fn resolve_style_for_classes_applies_font_family() {
    let mut world = World::new();
    let mut sheet = StyleSheet::default();

    sheet.set_class(
        "cjk-text",
        StyleSetter {
            font_family: Some(vec![
                "Primary Family".to_string(),
                "Fallback Family".to_string(),
            ]),
            ..StyleSetter::default()
        },
    );
    world.insert_resource(sheet);

    let resolved = crate::resolve_style_for_classes(&world, ["cjk-text"]);
    assert_eq!(
        resolved.font_family,
        Some(vec![
            "Primary Family".to_string(),
            "Fallback Family".to_string()
        ])
    );
}

#[test]
fn computed_style_lens_keeps_font_family_until_completion() {
    let mut world = World::new();

    let start = crate::ComputedStyle {
        font_family: Some(vec!["Family A".to_string()]),
        ..crate::ComputedStyle::default()
    };
    let end = crate::ComputedStyle {
        font_family: Some(vec!["Family B".to_string()]),
        ..crate::ComputedStyle::default()
    };

    let entity = world.spawn((start.clone(),)).id();
    let mut lens = crate::ComputedStyleLens {
        start: start.clone(),
        end: end.clone(),
    };

    {
        let target = world
            .get_mut::<crate::ComputedStyle>(entity)
            .expect("computed style should exist");
        lens.lerp(target, 0.5);
    }

    assert_eq!(
        world
            .get::<crate::ComputedStyle>(entity)
            .and_then(|style| style.font_family.clone()),
        Some(vec!["Family A".to_string()])
    );

    {
        let target = world
            .get_mut::<crate::ComputedStyle>(entity)
            .expect("computed style should exist");
        lens.lerp(target, 1.0);
    }

    assert_eq!(
        world
            .get::<crate::ComputedStyle>(entity)
            .and_then(|style| style.font_family.clone()),
        Some(vec!["Family B".to_string()])
    );
}

#[test]
fn xilem_font_bridge_deduplicates_same_font_bytes() {
    let mut bridge = crate::XilemFontBridge::default();
    assert!(bridge.register_font_bytes(b"font-data"));
    assert!(!bridge.register_font_bytes(b"font-data"));
}

#[test]
fn register_i18n_bundle_stores_locale_font_stacks_in_app_i18n() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .register_i18n_bundle(
            "en-US",
            SyncTextSource::String(include_str!("../../../assets/locales/en-US/main.ftl")),
            vec!["Inter", "sans-serif"],
        )
        .register_i18n_bundle(
            "zh-CN",
            SyncTextSource::String(include_str!("../../../assets/locales/zh-CN/main.ftl")),
            vec!["Inter", "Noto Sans CJK SC", "sans-serif"],
        );

    {
        let i18n = app.world().resource::<AppI18n>();
        assert_eq!(
            i18n.get_font_stack(),
            vec!["Inter".to_string(), "sans-serif".to_string()]
        );
    }

    app.world_mut().resource_mut::<AppI18n>().set_active_locale(
        "zh-CN"
            .parse()
            .expect("zh-CN locale identifier should parse"),
    );
    {
        let i18n = app.world().resource::<AppI18n>();
        assert_eq!(
            i18n.get_font_stack(),
            vec![
                "Inter".to_string(),
                "Noto Sans CJK SC".to_string(),
                "sans-serif".to_string()
            ]
        );
    }

    app.world_mut().resource_mut::<AppI18n>().set_active_locale(
        "ja-JP"
            .parse()
            .expect("ja-JP locale identifier should parse"),
    );
    assert_eq!(
        app.world().resource::<AppI18n>().get_font_stack(),
        vec!["Inter".to_string(), "sans-serif".to_string()]
    );
}

#[test]
fn resolve_localized_text_falls_back_when_cache_is_missing() {
    let mut world = World::new();
    let entity = world.spawn((crate::LocalizeText::new("hello_world"),)).id();

    let with_fallback = crate::resolve_localized_text(&world, entity, "Fallback");
    let without_fallback = crate::resolve_localized_text(&world, entity, "");

    assert_eq!(with_fallback, "Fallback");
    assert_eq!(without_fallback, "hello_world");
}

#[test]
fn ensure_overlay_root_spawns_once() {
    let mut world = World::new();
    world.spawn((UiRoot,));

    ensure_overlay_root(&mut world);
    ensure_overlay_root(&mut world);

    let mut overlay_query = world.query_filtered::<Entity, With<crate::UiOverlayRoot>>();
    let overlays = overlay_query.iter(&world).collect::<Vec<_>>();

    assert_eq!(overlays.len(), 1);
    assert!(world.get::<UiRoot>(overlays[0]).is_some());
}

#[test]
fn overlay_actions_toggle_and_select_combo_box() {
    let mut world = World::new();
    world.insert_resource(UiEventQueue::default());

    let overlay_root = world.spawn((UiRoot, crate::UiOverlayRoot)).id();
    let combo = world
        .spawn((crate::UiComboBox::new(vec![
            crate::UiComboOption::new("one", "One"),
            crate::UiComboOption::new("two", "Two"),
        ]),))
        .id();

    world
        .resource::<UiEventQueue>()
        .push_typed(combo, crate::OverlayUiAction::ToggleCombo);

    handle_overlay_actions(&mut world);

    let mut dropdown_query = world.query::<(Entity, &crate::AnchoredTo, &crate::UiDropdownMenu)>();
    let dropdowns = dropdown_query
        .iter(&world)
        .filter_map(|(entity, anchored_to, _)| (anchored_to.0 == combo).then_some(entity))
        .collect::<Vec<_>>();

    assert_eq!(dropdowns.len(), 1);
    let dropdown = dropdowns[0];
    assert!(
        world
            .get::<bevy_ecs::hierarchy::ChildOf>(dropdown)
            .is_some()
    );
    assert_eq!(
        world
            .get::<bevy_ecs::hierarchy::ChildOf>(dropdown)
            .expect("dropdown should be parented")
            .parent(),
        overlay_root
    );
    assert!(
        world
            .get::<crate::UiComboBox>(combo)
            .expect("combo should exist")
            .is_open
    );

    world.resource::<UiEventQueue>().push_typed(
        dropdown,
        crate::OverlayUiAction::SelectComboItem { index: 1 },
    );

    handle_overlay_actions(&mut world);

    let combo_after = world
        .get::<crate::UiComboBox>(combo)
        .expect("combo should exist");
    assert_eq!(combo_after.selected, 1);
    assert!(!combo_after.is_open);
    assert!(world.get_entity(dropdown).is_err());
}

#[test]
/// On HiDPI displays (scale_factor > 1) physical cursor coordinates are larger
/// than logical ones. `handle_global_overlay_clicks` must compare against logical
/// coordinates to match `OverlayBounds.content_rect` (which is in logical pixels).
///
/// This test verifies that a click whose *logical* position is inside the dropdown
/// content_rect does NOT dismiss the dropdown.
fn overlay_click_inside_logical_content_rect_not_dismissed_on_hidpi() {
    let mut world = World::new();
    world.insert_resource(ButtonInput::<MouseButton>::default());
    world.insert_resource(crate::OverlayStack::default());
    world.insert_resource(crate::OverlayPointerRoutingState::default());

    let mut window = Window::default();
    window.resolution.set(400.0, 300.0);
    window.resolution.set_scale_factor_override(Some(2.0));
    // Logical cursor position (150, 80) — INSIDE content_rect [100,300]×[50,200].
    // At scale_factor=2 the physical position would be (300, 160), which is OUTSIDE the
    // same rect, so this test distinguishes the logical vs physical code paths.
    window.set_cursor_position(Some(Vec2::new(150.0, 80.0)));
    world.spawn((window, PrimaryWindow));

    {
        let mut input = world.resource_mut::<ButtonInput<MouseButton>>();
        input.press(MouseButton::Left);
    }

    let anchor = world.spawn_empty().id();
    let dropdown = world
        .spawn((
            crate::UiDropdownMenu,
            crate::AnchoredTo(anchor),
            crate::OverlayState {
                is_modal: false,
                anchor: Some(anchor),
            },
            // content_rect in logical pixels: x=[100,300], y=[50,200]
            // Logical cursor (150, 80) is inside; physical (300, 160) would be outside.
            crate::OverlayBounds {
                content_rect: Rect::from_corners(
                    Vec2::new(100.0, 50.0),
                    Vec2::new(300.0, 200.0),
                ),
                trigger_rect: None,
            },
        ))
        .id();

    crate::sync_overlay_stack_lifecycle(&mut world);
    crate::handle_global_overlay_clicks(&mut world);

    // Logical (150,80) is inside content_rect, so the dropdown must NOT be dismissed.
    assert!(
        world.get_entity(dropdown).is_ok(),
        "dropdown was incorrectly despawned; logical cursor (150,80) is inside \
         content_rect [100,300]×[50,200] — the handler must use logical coordinates"
    );
}

#[test]
fn spawn_in_overlay_root_parents_entity_under_overlay_root() {
    let mut world = World::new();
    world.spawn((UiRoot,));

    let dialog = spawn_in_overlay_root(&mut world, (crate::UiDialog::new("title", "body"),));

    let overlay_root = ensure_overlay_root_entity(&mut world);
    let parent = world
        .get::<bevy_ecs::hierarchy::ChildOf>(dialog)
        .expect("dialog should be parented")
        .parent();

    assert_eq!(parent, overlay_root);
    assert!(world.get::<crate::UiOverlayRoot>(overlay_root).is_some());
}

#[test]
fn reparent_overlay_entities_moves_dialog_to_overlay_root() {
    let mut world = World::new();
    let app_root = world.spawn((UiRoot,)).id();
    let dialog = world
        .spawn((crate::UiDialog::new("title", "body"), ChildOf(app_root)))
        .id();

    reparent_overlay_entities(&mut world);

    let mut overlays = world.query_filtered::<Entity, With<crate::UiOverlayRoot>>();
    let overlay_root = overlays
        .iter(&world)
        .next()
        .expect("overlay root should exist");

    let parent = world
        .get::<bevy_ecs::hierarchy::ChildOf>(dialog)
        .expect("dialog should be parented")
        .parent();
    assert_eq!(parent, overlay_root);
}

#[test]
fn ensure_overlay_defaults_assigns_dialog_and_dropdown_configs() {
    let mut world = World::new();
    let combo = world
        .spawn((crate::UiComboBox::new(vec![crate::UiComboOption::new(
            "v", "V",
        )]),))
        .id();
    let dialog = world.spawn((crate::UiDialog::new("t", "b"),)).id();
    let dropdown = world
        .spawn((crate::UiDropdownMenu, crate::AnchoredTo(combo)))
        .id();

    ensure_overlay_defaults(&mut world);

    let dialog_config = world
        .get::<crate::OverlayConfig>(dialog)
        .expect("dialog should receive overlay config");
    assert_eq!(dialog_config.placement, crate::OverlayPlacement::Center);
    assert_eq!(dialog_config.anchor, None);
    assert!(!dialog_config.auto_flip);
    let dialog_state = world
        .get::<crate::OverlayState>(dialog)
        .expect("dialog should receive overlay state");
    assert!(dialog_state.is_modal);
    assert_eq!(dialog_state.anchor, None);
    let dialog_position = world
        .get::<crate::OverlayComputedPosition>(dialog)
        .expect("dialog should receive computed position");
    assert!(!dialog_position.is_positioned);
    assert!(world.get::<crate::OverlayBounds>(dialog).is_some());

    let dropdown_config = world
        .get::<crate::OverlayConfig>(dropdown)
        .expect("dropdown should receive overlay config");
    assert_eq!(
        dropdown_config.placement,
        crate::OverlayPlacement::BottomStart
    );
    assert_eq!(dropdown_config.anchor, Some(combo));
    assert!(dropdown_config.auto_flip);
    let dropdown_state = world
        .get::<crate::OverlayState>(dropdown)
        .expect("dropdown should receive overlay state");
    assert!(!dropdown_state.is_modal);
    assert_eq!(dropdown_state.anchor, Some(combo));
    let dropdown_position = world
        .get::<crate::OverlayComputedPosition>(dropdown)
        .expect("dropdown should receive computed position");
    assert!(!dropdown_position.is_positioned);
    assert!(world.get::<crate::OverlayBounds>(dropdown).is_some());
}

#[test]
fn sync_overlay_positions_uses_dynamic_primary_window_size_and_updates_bounds() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin);

    let mut window = Window::default();
    window.resolution.set(1024.0, 768.0);
    app.world_mut().spawn((window, PrimaryWindow));

    let dialog = app
        .world_mut()
        .spawn((crate::UiDialog::new("title", "body"),))
        .id();

    app.update();

    let initial = *app
        .world()
        .get::<crate::OverlayComputedPosition>(dialog)
        .expect("dialog should have computed position");
    let initial_bounds = app
        .world()
        .get::<crate::OverlayBounds>(dialog)
        .expect("dialog should have overlay bounds");

    assert!(initial_bounds.content_rect.min.x >= 0.0);
    assert!(initial_bounds.content_rect.min.y >= 0.0);
    assert!(initial_bounds.content_rect.max.x <= 1024.0 + f32::EPSILON);
    assert!(initial_bounds.content_rect.max.y <= 768.0 + f32::EPSILON);
    assert!(initial.is_positioned);

    {
        let world = app.world_mut();
        let mut query = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut primary_window = query
            .single_mut(world)
            .expect("primary window should exist");
        primary_window.resolution.set(1600.0, 900.0);
    }

    app.update();

    let resized = *app
        .world()
        .get::<crate::OverlayComputedPosition>(dialog)
        .expect("dialog should still have computed position");
    let resized_bounds = app
        .world()
        .get::<crate::OverlayBounds>(dialog)
        .expect("dialog should still have overlay bounds");

    assert!(resized.x > initial.x);
    assert_eq!(initial.width, resized.width);
    assert_eq!(initial.height, resized.height);
    assert!(resized.is_positioned);
    assert!(resized_bounds.content_rect.max.x <= 1600.0 + f32::EPSILON);
    assert!(resized_bounds.content_rect.max.y <= 900.0 + f32::EPSILON);
}

#[test]
fn sync_overlay_positions_works_without_primary_window_marker() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin);

    let mut window = Window::default();
    window.resolution.set(1280.0, 720.0);
    app.world_mut().spawn((window,));

    let dialog = app
        .world_mut()
        .spawn((crate::UiDialog::new("title", "body"),))
        .id();

    app.update();

    let computed = *app
        .world()
        .get::<crate::OverlayComputedPosition>(dialog)
        .expect("dialog should have computed position without PrimaryWindow marker");

    assert!(computed.width > 1.0);
    assert!(computed.height > 1.0);
    assert!(computed.x > 0.0);
    assert!(computed.y > 0.0);
    assert!(computed.is_positioned);
}

fn send_primary_click(app: &mut App, window_entity: Entity, position: Vec2) {
    {
        let world = app.world_mut();
        let mut query = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut primary_window = query
            .single_mut(world)
            .expect("primary window should exist");
        primary_window.set_cursor_position(Some(position));
    }

    app.world_mut().write_message(MouseButtonInput {
        button: MouseButton::Left,
        state: ButtonState::Pressed,
        window: window_entity,
    });
    app.world_mut().write_message(MouseButtonInput {
        button: MouseButton::Left,
        state: ButtonState::Released,
        window: window_entity,
    });

    app.update();
}

fn collect_widget_bounds_by_short_name(
    widget: WidgetRef<'_, dyn Widget>,
    short_type_name: &str,
    bounds: &mut Vec<Rect>,
) {
    for child in widget.children() {
        collect_widget_bounds_by_short_name(child, short_type_name, bounds);
    }

    if widget.short_type_name() == short_type_name {
        let ctx = widget.ctx();
        let origin = ctx.window_origin();
        let size = ctx.border_box_size();
        bounds.push(Rect::from_corners(
            Vec2::new(origin.x as f32, origin.y as f32),
            Vec2::new(
                (origin.x + size.width) as f32,
                (origin.y + size.height) as f32,
            ),
        ));
    }
}

#[test]
fn dialog_body_click_does_not_dismiss_overlay() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin);

    let mut window = Window::default();
    window.resolution.set(800.0, 600.0);
    window.set_cursor_position(Some(Vec2::new(0.0, 0.0)));
    let window_entity = app.world_mut().spawn((window, PrimaryWindow)).id();

    let dialog = spawn_in_overlay_root(&mut app.world_mut(), (crate::UiDialog::new("t", "b"),));

    app.update();

    let content_rect = app
        .world()
        .get::<crate::OverlayBounds>(dialog)
        .expect("dialog should have overlay bounds")
        .content_rect;

    let click_position = Vec2::new(
        (content_rect.min.x + content_rect.max.x) * 0.5,
        content_rect.min.y + 24.0,
    );

    send_primary_click(&mut app, window_entity, click_position);

    assert!(app.world().get_entity(dialog).is_ok());
}

#[test]
fn dialog_dismiss_button_targets_dialog_entity() {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin);

    let mut window = Window::default();
    window.resolution.set(800.0, 600.0);
    window.set_cursor_position(Some(Vec2::new(0.0, 0.0)));
    app.world_mut().spawn((window, PrimaryWindow));

    let dialog = spawn_in_overlay_root(&mut app.world_mut(), (crate::UiDialog::new("t", "b"),));

    app.update();

    let content_rect = app
        .world()
        .get::<crate::OverlayBounds>(dialog)
        .expect("dialog should have overlay bounds")
        .content_rect;

    let button_rect = {
        let runtime = app.world().non_send_resource::<crate::MasonryRuntime>();
        let root = runtime.render_root.get_layer_root(0);
        let mut button_rects = Vec::new();
        collect_widget_bounds_by_short_name(root, "EcsButtonWidget", &mut button_rects);

        button_rects
            .into_iter()
            .filter(|rect| {
                let width = rect.max.x - rect.min.x;
                let height = rect.max.y - rect.min.y;
                width < (content_rect.max.x - content_rect.min.x)
                    && height < (content_rect.max.y - content_rect.min.y)
            })
            .min_by(|a, b| {
                let area_a = (a.max.x - a.min.x) * (a.max.y - a.min.y);
                let area_b = (b.max.x - b.min.x) * (b.max.y - b.min.y);
                area_a.total_cmp(&area_b)
            })
            .expect("dialog should project a dedicated dismiss button")
    };

    let click_position = Vec2::new(
        (button_rect.min.x + button_rect.max.x) * 0.5,
        (button_rect.min.y + button_rect.max.y) * 0.5,
    );

    let (hit_widget, hit_debug_text) = {
        let runtime = app.world().non_send_resource::<crate::MasonryRuntime>();
        let root = runtime.render_root.get_layer_root(0);
        root.find_widget_under_pointer((click_position.x as f64, click_position.y as f64).into())
            .map(|widget| {
                (
                    widget.short_type_name().to_string(),
                    widget.get_debug_text().unwrap_or_default(),
                )
            })
            .unwrap_or_default()
    };

    assert_eq!(hit_widget.as_str(), "EcsButtonWidget");
    assert_eq!(hit_debug_text, format!("entity={}", dialog.to_bits()));
}

#[test]
fn overlay_action_dismiss_dialog_despawns_dialog() {
    let mut world = World::new();
    world.insert_resource(UiEventQueue::default());

    let dialog = world.spawn((crate::UiDialog::new("title", "body"),)).id();

    world
        .resource::<UiEventQueue>()
        .push_typed(dialog, crate::OverlayUiAction::DismissDialog);

    handle_overlay_actions(&mut world);

    assert!(world.get_entity(dialog).is_err());
}

#[test]
fn native_dismiss_overlays_on_click_closes_only_outside_bounds_and_anchor() {
    let mut world = World::new();
    world.insert_resource(ButtonInput::<MouseButton>::default());
    world.insert_resource(crate::OverlayStack::default());

    let mut window = Window::default();
    window.resolution.set(800.0, 600.0);
    window.set_cursor_position(Some(Vec2::new(240.0, 120.0)));
    world.spawn((window, PrimaryWindow));

    {
        let mut input = world.resource_mut::<ButtonInput<MouseButton>>();
        input.press(MouseButton::Left);
    }

    let anchor = world.spawn_empty().id();
    let dropdown = world
        .spawn((
            crate::UiDropdownMenu,
            crate::AnchoredTo(anchor),
            crate::OverlayState {
                is_modal: false,
                anchor: Some(anchor),
            },
            crate::OverlayBounds {
                content_rect: Rect::from_corners(Vec2::new(100.0, 100.0), Vec2::new(200.0, 200.0)),
                trigger_rect: Some(Rect::from_corners(
                    Vec2::new(220.0, 100.0),
                    Vec2::new(300.0, 130.0),
                )),
            },
        ))
        .id();

    crate::native_dismiss_overlays_on_click(&mut world);
    assert!(world.get_entity(dropdown).is_ok());

    {
        let mut window_query = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut primary_window = window_query
            .single_mut(&mut world)
            .expect("primary window should exist");
        primary_window.set_cursor_position(Some(Vec2::new(500.0, 500.0)));
    }
    {
        let mut input = world.resource_mut::<ButtonInput<MouseButton>>();
        input.release(MouseButton::Left);
        input.clear();
        input.press(MouseButton::Left);
    }

    crate::native_dismiss_overlays_on_click(&mut world);
    assert!(world.get_entity(dropdown).is_err());
}

#[test]
/// Previously this function resolved to physical cursor coordinates; it now resolves
/// to logical cursor coordinates so that the comparison against `content_rect`
/// (which is always in logical pixels) works correctly on HiDPI displays.
///
/// Logical cursor (120, 60) is OUTSIDE content_rect [200,300]×[100,200] in logical space,
/// so the dropdown should be dismissed (outside-click behavior).
fn native_dismiss_overlays_on_click_uses_logical_cursor_for_inside_hit_checks() {
    let mut world = World::new();
    world.insert_resource(ButtonInput::<MouseButton>::default());
    world.insert_resource(crate::OverlayStack::default());
    world.insert_resource(crate::OverlayPointerRoutingState::default());

    let mut window = Window::default();
    window.resolution.set(800.0, 600.0);
    window.resolution.set_scale_factor_override(Some(2.0));
    // Logical cursor at (120, 60) — outside content_rect [200,300]×[100,200].
    window.set_cursor_position(Some(Vec2::new(120.0, 60.0)));
    world.spawn((window, PrimaryWindow));

    {
        let mut input = world.resource_mut::<ButtonInput<MouseButton>>();
        input.press(MouseButton::Left);
    }

    let anchor = world.spawn_empty().id();
    let dropdown = world
        .spawn((
            crate::UiDropdownMenu,
            crate::AnchoredTo(anchor),
            crate::OverlayState {
                is_modal: false,
                anchor: Some(anchor),
            },
            crate::OverlayBounds {
                content_rect: Rect::from_corners(Vec2::new(200.0, 100.0), Vec2::new(300.0, 200.0)),
                trigger_rect: Some(Rect::from_corners(
                    Vec2::new(200.0, 100.0),
                    Vec2::new(300.0, 200.0),
                )),
            },
        ))
        .id();

    crate::sync_overlay_stack_lifecycle(&mut world);
    crate::native_dismiss_overlays_on_click(&mut world);

    // Logical (120,60) is outside [200,300]×[100,200], so the outside-click handler
    // should have dismissed the dropdown.
    assert!(
        world.get_entity(dropdown).is_err(),
        "click outside content_rect in logical coords should dismiss the dropdown"
    );
}

#[test]
fn native_dismiss_overlays_on_click_closes_nested_topmost_overlay_first() {
    let mut world = World::new();
    world.insert_resource(ButtonInput::<MouseButton>::default());
    world.insert_resource(crate::OverlayStack::default());
    world.insert_resource(crate::OverlayPointerRoutingState::default());

    let mut window = Window::default();
    window.resolution.set(800.0, 600.0);
    window.set_cursor_position(Some(Vec2::new(500.0, 180.0)));
    world.spawn((window, PrimaryWindow));

    let dialog = world
        .spawn((
            crate::UiDialog::new("modal", "body"),
            crate::OverlayState {
                is_modal: true,
                anchor: None,
            },
            crate::OverlayBounds {
                content_rect: Rect::from_corners(Vec2::new(150.0, 120.0), Vec2::new(650.0, 500.0)),
                trigger_rect: None,
            },
        ))
        .id();

    let combo_anchor = world.spawn_empty().id();

    let dropdown = world
        .spawn((
            crate::UiDropdownMenu,
            crate::AnchoredTo(combo_anchor),
            crate::OverlayState {
                is_modal: false,
                anchor: Some(combo_anchor),
            },
            crate::OverlayBounds {
                content_rect: Rect::from_corners(Vec2::new(200.0, 220.0), Vec2::new(360.0, 340.0)),
                trigger_rect: Some(Rect::from_corners(
                    Vec2::new(180.0, 160.0),
                    Vec2::new(340.0, 196.0),
                )),
            },
        ))
        .id();

    crate::sync_overlay_stack_lifecycle(&mut world);

    {
        let stack = world.resource::<crate::OverlayStack>();
        assert_eq!(stack.active_overlays, vec![dialog, dropdown]);
    }

    {
        let mut input = world.resource_mut::<ButtonInput<MouseButton>>();
        input.press(MouseButton::Left);
    }

    crate::native_dismiss_overlays_on_click(&mut world);

    assert!(world.get_entity(dropdown).is_err());
    assert!(world.get_entity(dialog).is_ok());
    {
        let stack = world.resource::<crate::OverlayStack>();
        assert_eq!(stack.active_overlays, vec![dialog]);
    }

    {
        let mut query = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut primary = query
            .single_mut(&mut world)
            .expect("primary window should exist");
        primary.set_cursor_position(Some(Vec2::new(60.0, 60.0)));
    }
    {
        let mut input = world.resource_mut::<ButtonInput<MouseButton>>();
        input.release(MouseButton::Left);
        input.clear();
        input.press(MouseButton::Left);
    }

    crate::native_dismiss_overlays_on_click(&mut world);

    assert!(world.get_entity(dialog).is_err());
    let stack = world.resource::<crate::OverlayStack>();
    assert!(stack.active_overlays.is_empty());
}

#[test]
fn native_dismiss_overlays_on_click_works_without_primary_window_marker() {
    let mut world = World::new();
    world.insert_resource(ButtonInput::<MouseButton>::default());
    world.insert_resource(crate::OverlayStack::default());

    let mut window = Window::default();
    window.resolution.set(800.0, 600.0);
    window.set_cursor_position(Some(Vec2::new(790.0, 590.0)));
    world.spawn((window,));

    {
        let mut input = world.resource_mut::<ButtonInput<MouseButton>>();
        input.press(MouseButton::Left);
    }

    let dialog = world
        .spawn((
            crate::UiDialog::new("title", "body"),
            crate::OverlayState {
                is_modal: true,
                anchor: None,
            },
            crate::OverlayBounds {
                content_rect: Rect::from_corners(Vec2::new(100.0, 100.0), Vec2::new(300.0, 260.0)),
                trigger_rect: None,
            },
        ))
        .id();

    crate::native_dismiss_overlays_on_click(&mut world);

    assert!(world.get_entity(dialog).is_err());
}

#[test]
fn native_dismiss_overlays_on_click_logs_when_window_missing() {
    init_test_tracing();

    let mut world = World::new();
    world.insert_resource(ButtonInput::<MouseButton>::default());

    {
        let mut input = world.resource_mut::<ButtonInput<MouseButton>>();
        input.press(MouseButton::Left);
    }

    let dialog = world
        .spawn((
            crate::UiDialog::new("title", "body"),
            crate::OverlayState {
                is_modal: true,
                anchor: None,
            },
            crate::OverlayBounds {
                content_rect: Rect::from_corners(Vec2::new(100.0, 100.0), Vec2::new(300.0, 260.0)),
                trigger_rect: None,
            },
        ))
        .id();

    crate::native_dismiss_overlays_on_click(&mut world);

    assert!(world.get_entity(dialog).is_ok());
}

#[test]
fn pointer_hits_bubble_to_parent_until_consumed() {
    let mut world = World::new();
    world.insert_resource(UiEventQueue::default());

    let root = world.spawn_empty().id();
    let parent = world
        .spawn((ChildOf(root), crate::StopUiPointerPropagation))
        .id();
    let child = world.spawn((ChildOf(parent),)).id();

    world.resource::<UiEventQueue>().push_typed(
        child,
        crate::UiPointerHitEvent {
            target: child,
            position: (12.0, 24.0),
            button: MouseButton::Left,
            phase: crate::UiPointerPhase::Pressed,
        },
    );

    bubble_ui_pointer_events(&mut world);

    let bubbled = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<crate::UiPointerEvent>();

    assert_eq!(bubbled.len(), 2);
    assert_eq!(bubbled[0].entity, child);
    assert_eq!(bubbled[0].action.current_target, child);
    assert!(!bubbled[0].action.consumed);

    assert_eq!(bubbled[1].entity, parent);
    assert_eq!(bubbled[1].action.current_target, parent);
    assert!(bubbled[1].action.consumed);
}
