use std::{path::PathBuf, sync::Arc, time::Duration};

use crate::{
    ActiveLocale, AppBevyXilemExt, BevyXilemPlugin, ColorStyle, Hovered, LocaleFontRegistry,
    LocalizationCache, Pressed, ProjectionCtx, Selector, StyleRule, StyleSetter, StyleSheet,
    UiEventQueue, UiProjectorRegistry, UiRoot, UiView, ecs_button, register_builtin_projectors,
    resolve_style, resolve_style_for_entity_classes, synthesize_roots_with_stats,
};
use bevy_app::App;
use bevy_asset::{AssetPlugin, Assets};
use bevy_ecs::{hierarchy::ChildOf, prelude::*};
use bevy_fluent::prelude::BundleAsset;
use bevy_tasks::{IoTaskPool, TaskPool};
use bevy_tweening::Lens;

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
fn plugin_adds_fluent_assets_when_asset_plugin_exists() {
    let mut app = App::new();
    app.add_plugins((AssetPlugin::default(), BevyXilemPlugin));

    assert!(app.world().contains_resource::<Assets<BundleAsset>>());
}

#[test]
fn localization_cache_resolves_showcase_hello_world_for_zh_cn() {
    let _ = IoTaskPool::get_or_init(TaskPool::new);

    let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("assets");
    assert!(
        assets_dir.join("locales/zh-CN/main.ftl").exists(),
        "expected showcase locale file at {}",
        assets_dir.join("locales/zh-CN/main.ftl").display()
    );

    let mut app = App::new();
    app.add_plugins((
        AssetPlugin {
            file_path: assets_dir.to_string_lossy().into_owned(),
            ..AssetPlugin::default()
        },
        BevyXilemPlugin,
    ));
    app.insert_resource(ActiveLocale::new(
        "zh-CN"
            .parse()
            .expect("zh-CN locale identifier should parse"),
    ));

    let mut translated = None;
    for _ in 0..300 {
        app.update();

        translated = app
            .world()
            .resource::<LocalizationCache>()
            .content("hello_world");

        if translated.is_some() {
            break;
        }

        std::thread::sleep(Duration::from_millis(5));
    }

    assert_eq!(translated.as_deref(), Some("你好，世界！"));
}

#[test]
fn resolve_localized_text_prefers_translation_over_uilabel_fallback() {
    let _ = IoTaskPool::get_or_init(TaskPool::new);

    let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("assets");

    let mut app = App::new();
    app.add_plugins((
        AssetPlugin {
            file_path: assets_dir.to_string_lossy().into_owned(),
            ..AssetPlugin::default()
        },
        BevyXilemPlugin,
    ));
    app.insert_resource(ActiveLocale::new(
        "zh-CN"
            .parse()
            .expect("zh-CN locale identifier should parse"),
    ));

    let entity = app
        .world_mut()
        .spawn((
            crate::UiLabel::new("Hello world"),
            crate::LocalizeText::new("hello_world"),
        ))
        .id();

    let mut resolved = String::from("Hello world");
    for _ in 0..300 {
        app.update();

        resolved = crate::resolve_localized_text(app.world(), entity, "Hello world");
        if resolved == "你好，世界！" {
            break;
        }

        std::thread::sleep(Duration::from_millis(5));
    }

    assert_eq!(resolved, "你好，世界！");
}

#[test]
fn localized_text_updates_after_active_locale_change() {
    let _ = IoTaskPool::get_or_init(TaskPool::new);

    let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("assets");

    let mut app = App::new();
    app.add_plugins((
        AssetPlugin {
            file_path: assets_dir.to_string_lossy().into_owned(),
            ..AssetPlugin::default()
        },
        BevyXilemPlugin,
    ));
    app.insert_resource(ActiveLocale::new(
        "en-US"
            .parse()
            .expect("en-US locale identifier should parse"),
    ));

    let entity = app
        .world_mut()
        .spawn((
            crate::UiLabel::new("Hello world"),
            crate::LocalizeText::new("hello_world"),
        ))
        .id();

    let mut resolved_en = String::from("Hello world");
    for _ in 0..300 {
        app.update();

        resolved_en = crate::resolve_localized_text(app.world(), entity, "Hello world");
        if resolved_en == "Hello, world!" {
            break;
        }

        std::thread::sleep(Duration::from_millis(5));
    }

    assert_eq!(resolved_en, "Hello, world!");

    app.world_mut().resource_mut::<ActiveLocale>().0 = "zh-CN"
        .parse()
        .expect("zh-CN locale identifier should parse");

    let mut resolved_zh = resolved_en;
    for _ in 0..300 {
        app.update();

        resolved_zh = crate::resolve_localized_text(app.world(), entity, "Hello world");
        if resolved_zh == "你好，世界！" {
            break;
        }

        std::thread::sleep(Duration::from_millis(5));
    }

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
fn locale_font_registry_resolves_mapping_then_default() {
    let registry = LocaleFontRegistry::default()
        .set_default(vec!["Default Sans", "sans-serif"])
        .add_mapping("fr-FR", vec!["French Sans", "sans-serif"]);

    let french = "fr-FR"
        .parse()
        .expect("fr-FR locale identifier should parse");
    let english = "en-US"
        .parse()
        .expect("en-US locale identifier should parse");

    assert_eq!(
        registry.font_stack_for_locale(&french),
        Some(vec!["French Sans".to_string(), "sans-serif".to_string()])
    );
    assert_eq!(
        registry.font_stack_for_locale(&english),
        Some(vec!["Default Sans".to_string(), "sans-serif".to_string()])
    );
}

#[test]
fn locale_font_fallback_uses_registry_and_respects_explicit_style_font() {
    let mut world = World::new();
    world.insert_resource(ActiveLocale::new(
        "fr-FR"
            .parse()
            .expect("fr-FR locale identifier should parse"),
    ));
    world.insert_resource(
        LocaleFontRegistry::default()
            .set_default(vec!["Default Sans", "sans-serif"])
            .add_mapping("fr-FR", vec!["French Sans", "sans-serif"]),
    );

    let mut resolved = crate::ResolvedStyle::default();
    crate::apply_locale_font_family_fallback(&world, &mut resolved);
    assert_eq!(
        resolved.font_family,
        Some(vec!["French Sans".to_string(), "sans-serif".to_string()])
    );

    let mut explicit = crate::ResolvedStyle {
        font_family: Some(vec!["App Font".to_string()]),
        ..crate::ResolvedStyle::default()
    };
    crate::apply_locale_font_family_fallback(&world, &mut explicit);
    assert_eq!(explicit.font_family, Some(vec!["App Font".to_string()]));
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
