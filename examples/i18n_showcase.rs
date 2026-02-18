use std::sync::Arc;

use bevy_embedded_assets::{EmbeddedAssetPlugin, PluginMode};
use bevy_xilem::{
    AppBevyXilemExt, AppI18n, BevyXilemPlugin, BuiltinUiAction, ColorStyle, LayoutStyle,
    LocaleFontRegistry, LocalizeText, ProjectionCtx, StyleClass, StyleSetter, StyleSheet,
    SyncAssetSource, SyncTextSource, TextStyle, UiButton, UiEventQueue, UiFlexColumn, UiLabel,
    UiRoot, UiView, apply_label_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_asset::AssetPlugin,
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    bevy_tasks::{IoTaskPool, TaskPool},
    bevy_text::TextPlugin,
    resolve_style, run_app_with_window_options,
    xilem::{
        Color,
        view::label,
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};
use unic_langid::LanguageIdentifier;

#[derive(Resource, Debug, Clone, Copy)]
struct I18nRuntime {
    toggle_button: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
struct LocaleBadge;

fn parse_locale(tag: &str) -> LanguageIdentifier {
    tag.parse()
        .unwrap_or_else(|_| panic!("locale `{tag}` should parse"))
}

fn ensure_task_pool_initialized() {
    let _ = IoTaskPool::get_or_init(TaskPool::new);
}

fn configure_locale_font_registry() -> LocaleFontRegistry {
    LocaleFontRegistry::default()
        .set_default(vec![
            "Inter",
            "Noto Sans SC",
            "Noto Sans CJK SC",
            "Noto Sans JP",
            "Noto Sans CJK JP",
            "sans-serif",
        ])
        .add_mapping(
            "ja-JP",
            vec![
                "Inter",
                "Noto Sans JP",
                "Noto Sans CJK JP",
                "Noto Sans SC",
                "Noto Sans CJK SC",
                "sans-serif",
            ],
        )
        .add_mapping(
            "zh-CN",
            vec![
                "Inter",
                "Noto Sans SC",
                "Noto Sans CJK SC",
                "Noto Sans JP",
                "Noto Sans CJK JP",
                "sans-serif",
            ],
        )
}

fn project_locale_badge(_: &LocaleBadge, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let locale_text = ctx.world.get_resource::<AppI18n>().map_or_else(
        || "en-US".to_string(),
        |i18n| i18n.active_locale.to_string(),
    );

    Arc::new(apply_widget_style(
        apply_label_style(label(format!("Active locale: {locale_text}")), &style),
        &style,
    ))
}

fn setup_i18n_world(mut commands: Commands) {
    let root = commands
        .spawn((
            UiRoot,
            UiFlexColumn,
            StyleClass(vec!["i18n.root".to_string()]),
        ))
        .id();

    commands.spawn((
        UiLabel::new("Hello world"),
        LocalizeText::new("hello_world"),
        StyleClass(vec!["i18n.title".to_string()]),
        ChildOf(root),
    ));

    commands.spawn((
        UiLabel::new("Han unification sample"),
        LocalizeText::new("han_unification_test"),
        StyleClass(vec!["i18n.han".to_string()]),
        ChildOf(root),
    ));

    commands.spawn((
        LocaleBadge,
        StyleClass(vec!["i18n.badge".to_string()]),
        ChildOf(root),
    ));

    let toggle_button = commands
        .spawn((
            UiButton::new("Change Language"),
            LocalizeText::new("toggle_language"),
            StyleClass(vec!["i18n.toggle".to_string()]),
            ChildOf(root),
        ))
        .id();

    commands.insert_resource(I18nRuntime { toggle_button });
}

fn setup_i18n_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "i18n.root",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(24.0),
                gap: Some(14.0),
                corner_radius: Some(12.0),
                border_width: Some(1.0),
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x14, 0x18, 0x22)),
                border: Some(Color::from_rgb8(0x2A, 0x35, 0x4C)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "i18n.title",
        StyleSetter {
            text: TextStyle { size: Some(28.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xE8, 0xF0, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "i18n.han",
        StyleSetter {
            text: TextStyle { size: Some(44.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "i18n.badge",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            layout: LayoutStyle {
                padding: Some(8.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x1C, 0x24, 0x36)),
                border: Some(Color::from_rgb8(0x3E, 0x4F, 0x73)),
                text: Some(Color::from_rgb8(0xCD, 0xDD, 0xFA)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "i18n.toggle",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(8.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2A, 0x61, 0xE2)),
                hover_bg: Some(Color::from_rgb8(0x1E, 0x52, 0xCC)),
                pressed_bg: Some(Color::from_rgb8(0x1A, 0x45, 0xA8)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );
}

fn next_locale(current: &LanguageIdentifier) -> LanguageIdentifier {
    if current.language.as_str() == "ja" {
        parse_locale("en-US")
    } else if current.language.as_str() == "zh"
        && current
            .region
            .is_some_and(|region| region.as_str().eq_ignore_ascii_case("CN"))
    {
        parse_locale("ja-JP")
    } else {
        parse_locale("zh-CN")
    }
}

fn drain_i18n_events(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<BuiltinUiAction>();

    if events.is_empty() {
        return;
    }

    let runtime = *world.resource::<I18nRuntime>();

    for event in events {
        if event.entity != runtime.toggle_button {
            continue;
        }

        if !matches!(event.action, BuiltinUiAction::Clicked) {
            continue;
        }

        let next = {
            let current = world.resource::<AppI18n>().active_locale.clone();
            next_locale(&current)
        };

        world.resource_mut::<AppI18n>().set_active_locale(next);
    }
}

fn build_i18n_app() -> App {
    ensure_task_pool_initialized();

    let mut app = App::new();

    app.add_plugins((
        EmbeddedAssetPlugin {
            mode: PluginMode::ReplaceDefault,
        },
        AssetPlugin::default(),
        TextPlugin::default(),
        BevyXilemPlugin,
    ))
    .insert_resource(AppI18n::new(parse_locale("en-US")))
    .register_xilem_font(SyncAssetSource::FilePath("assets/fonts/Inter-Regular.otf"))
    .register_xilem_font(SyncAssetSource::FilePath(
        "assets/fonts/NotoSansCJKsc-Regular.otf",
    ))
    .register_xilem_font(SyncAssetSource::FilePath(
        "assets/fonts/NotoSansCJKjp-Regular.otf",
    ))
    .register_i18n_bundle(
        "en-US",
        SyncTextSource::FilePath("assets/locales/en-US/main.ftl"),
    )
    .register_i18n_bundle(
        "zh-CN",
        SyncTextSource::FilePath("assets/locales/zh-CN/main.ftl"),
    )
    .register_i18n_bundle(
        "ja-JP",
        SyncTextSource::FilePath("assets/locales/ja-JP/main.ftl"),
    )
    .insert_resource(configure_locale_font_registry())
    .register_projector::<LocaleBadge>(project_locale_badge)
    .add_systems(Startup, (setup_i18n_styles, setup_i18n_world))
    .add_systems(PreUpdate, drain_i18n_events);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_i18n_app(), "i18n Showcase", |options| {
        options.with_initial_inner_size(LogicalSize::new(960.0, 520.0))
    })
}
