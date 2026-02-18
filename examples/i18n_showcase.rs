mod utils;

use std::sync::Arc;

use bevy_embedded_assets::{EmbeddedAssetPlugin, PluginMode};
use bevy_xilem::{
    AppBevyXilemExt, AppI18n, BevyXilemPlugin, BuiltinUiAction, ColorStyle, LayoutStyle,
    LocalizeText, OverlayConfig, OverlayPlacement, ProjectionCtx, StyleClass, StyleSetter,
    StyleSheet, SyncAssetSource, SyncTextSource, TextStyle, UiButton, UiComboBox,
    UiComboBoxChanged, UiComboOption, UiDialog, UiEventQueue, UiFlexColumn, UiLabel, UiRoot,
    UiView, apply_label_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_asset::AssetPlugin,
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    bevy_tasks::{IoTaskPool, TaskPool},
    bevy_text::TextPlugin,
    resolve_style, run_app_with_window_options, spawn_in_overlay_root,
    xilem::{
        Color,
        view::label,
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};
use unic_langid::LanguageIdentifier;
use utils::init_logging;

#[derive(Resource, Debug, Clone, Copy)]
struct I18nRuntime {
    locale_combo: Entity,
    edge_button: Entity,
    combo_box: Entity,
    show_modal_button: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
struct LocaleBadge;

#[derive(Component, Debug, Clone, Copy)]
struct ShowcaseStatus;

#[derive(Resource, Debug, Clone, Default)]
struct ShowcaseState {
    edge_clicks: u32,
    selected_combo_value: String,
}

fn parse_locale(tag: &str) -> LanguageIdentifier {
    tag.parse()
        .unwrap_or_else(|_| panic!("locale `{tag}` should parse"))
}

fn ensure_task_pool_initialized() {
    let _ = IoTaskPool::get_or_init(TaskPool::new);
}

fn cjk_fallback_font_stack() -> Vec<&'static str> {
    vec![
        "Inter",
        "Noto Sans CJK SC",
        "NotoSansCJKsc",
        "Noto Sans CJK JP",
        "NotoSansCJKjp",
        "PingFang SC",
        "Hiragino Sans",
        "Apple SD Gothic Neo",
        "sans-serif",
    ]
}

fn zh_cjk_fallback_font_stack() -> Vec<&'static str> {
    vec![
        "Inter",
        "Noto Sans CJK SC",
        "NotoSansCJKsc",
        "Noto Sans CJK JP",
        "NotoSansCJKjp",
        "PingFang SC",
        "Hiragino Sans",
        "Apple SD Gothic Neo",
        "sans-serif",
    ]
}

fn ja_cjk_fallback_font_stack() -> Vec<&'static str> {
    vec![
        "Inter",
        "Noto Sans CJK JP",
        "NotoSansCJKjp",
        "Noto Sans CJK SC",
        "NotoSansCJKsc",
        "Hiragino Sans",
        "PingFang SC",
        "Apple SD Gothic Neo",
        "sans-serif",
    ]
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

fn project_showcase_status(_: &ShowcaseStatus, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let state = ctx
        .world
        .get_resource::<ShowcaseState>()
        .cloned()
        .unwrap_or_default();

    let edge_prefix = ctx.world.get_resource::<AppI18n>().map_or_else(
        || "Edge clicks".to_string(),
        |i18n| i18n.translate("showcase-status-edge-clicks"),
    );
    let combo_prefix = ctx.world.get_resource::<AppI18n>().map_or_else(
        || "Combo value".to_string(),
        |i18n| i18n.translate("showcase-status-combo-value"),
    );

    let text = format!(
        "{edge_prefix}: {}\n{combo_prefix}: {}",
        state.edge_clicks,
        if state.selected_combo_value.is_empty() {
            "-"
        } else {
            &state.selected_combo_value
        }
    );

    Arc::new(apply_widget_style(
        apply_label_style(label(text), &style),
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

    let edge_button = commands
        .spawn((
            UiButton::new("Edge hit test button"),
            LocalizeText::new("showcase-edge-button"),
            StyleClass(vec!["i18n.edge-button".to_string()]),
            ChildOf(root),
        ))
        .id();

    let combo_box = commands
        .spawn((
            UiComboBox::new(vec![
                UiComboOption::new("inter", "Inter").with_label_key("showcase-combo-option-inter"),
                UiComboOption::new("noto-sc", "Noto Sans CJK SC")
                    .with_label_key("showcase-combo-option-sc"),
                UiComboOption::new("noto-jp", "Noto Sans CJK JP")
                    .with_label_key("showcase-combo-option-jp"),
            ])
            .with_placeholder_key("showcase-combo-placeholder")
            .with_overlay_placement(OverlayPlacement::BottomStart)
            .with_overlay_auto_flip(true),
            StyleClass(vec!["i18n.combo".to_string()]),
            ChildOf(root),
        ))
        .id();

    let show_modal_button = commands
        .spawn((
            UiButton::new("Show modal"),
            LocalizeText::new("showcase-show-modal"),
            StyleClass(vec!["i18n.show-modal".to_string()]),
            ChildOf(root),
        ))
        .id();

    commands.spawn((
        ShowcaseStatus,
        StyleClass(vec!["i18n.status".to_string()]),
        ChildOf(root),
    ));

    let locale_combo = commands
        .spawn((
            UiComboBox::new(vec![
                UiComboOption::new("en-US", "English"),
                UiComboOption::new("zh-CN", "简体中文"),
                UiComboOption::new("ja-JP", "日本語"),
            ])
            .with_placeholder("Language")
            .with_overlay_placement(OverlayPlacement::BottomStart)
            .with_overlay_auto_flip(true),
            StyleClass(vec!["i18n.combo".to_string()]),
            ChildOf(root),
        ))
        .id();

    commands.insert_resource(I18nRuntime {
        locale_combo,
        edge_button,
        combo_box,
        show_modal_button,
    });
    commands.insert_resource(ShowcaseState::default());
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
        "i18n.edge-button",
        StyleSetter {
            text: TextStyle { size: Some(20.0) },
            layout: LayoutStyle {
                padding: Some(26.0),
                corner_radius: Some(16.0),
                border_width: Some(2.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2D, 0x7A, 0x5A)),
                hover_bg: Some(Color::from_rgb8(0x36, 0x8A, 0x68)),
                pressed_bg: Some(Color::from_rgb8(0x24, 0x66, 0x4B)),
                border: Some(Color::from_rgb8(0x72, 0xD8, 0xAF)),
                text: Some(Color::from_rgb8(0xEC, 0xFF, 0xF8)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "i18n.combo",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            layout: LayoutStyle {
                padding: Some(12.0),
                corner_radius: Some(10.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x1F, 0x2A, 0x40)),
                hover_bg: Some(Color::from_rgb8(0x27, 0x35, 0x52)),
                pressed_bg: Some(Color::from_rgb8(0x17, 0x23, 0x38)),
                border: Some(Color::from_rgb8(0x4A, 0x5F, 0x8A)),
                text: Some(Color::from_rgb8(0xDE, 0xE8, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "i18n.show-modal",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(12.0),
                corner_radius: Some(10.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x8A, 0x43, 0xFF)),
                hover_bg: Some(Color::from_rgb8(0x99, 0x56, 0xFF)),
                pressed_bg: Some(Color::from_rgb8(0x76, 0x36, 0xD9)),
                text: Some(Color::from_rgb8(0xF8, 0xF2, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "i18n.status",
        StyleSetter {
            text: TextStyle { size: Some(15.0) },
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x1A, 0x21, 0x31)),
                border: Some(Color::from_rgb8(0x3D, 0x4C, 0x6A)),
                text: Some(Color::from_rgb8(0xD0, 0xDE, 0xF8)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class("i18n.dialog", StyleSetter::default());

    style_sheet.set_class(
        "overlay.dialog.backdrop",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(0x00, 0x00, 0x00, 0xA0)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "overlay.dialog.title",
        StyleSetter {
            text: TextStyle { size: Some(24.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xF2, 0xF6, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "overlay.dialog.body",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xC6, 0xD3, 0xF0)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "overlay.dialog.dismiss",
        StyleSetter {
            text: TextStyle { size: Some(15.0) },
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(8.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x3F, 0x68, 0xE9)),
                hover_bg: Some(Color::from_rgb8(0x4D, 0x77, 0xF3)),
                pressed_bg: Some(Color::from_rgb8(0x2D, 0x56, 0xD3)),
                text: Some(Color::from_rgb8(0xF1, 0xF6, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "overlay.dropdown.menu",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(6.0),
                gap: Some(6.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x18, 0x22, 0x35)),
                border: Some(Color::from_rgb8(0x46, 0x59, 0x84)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "overlay.dropdown.item",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(6.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x26, 0x33, 0x4D)),
                hover_bg: Some(Color::from_rgb8(0x31, 0x40, 0x5E)),
                pressed_bg: Some(Color::from_rgb8(0x20, 0x2C, 0x44)),
                text: Some(Color::from_rgb8(0xE0, 0xEA, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "overlay.dropdown.backdrop",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(0x00, 0x00, 0x00, 0x00)),
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

fn drain_i18n_events(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<BuiltinUiAction>();

    let runtime = *world.resource::<I18nRuntime>();

    for event in events {
        if !matches!(event.action, BuiltinUiAction::Clicked) {
            continue;
        }

        if event.entity == runtime.edge_button {
            world.resource_mut::<ShowcaseState>().edge_clicks += 1;
            continue;
        }

        if event.entity == runtime.show_modal_button {
            spawn_in_overlay_root(world, (
                UiDialog::new(
                    "Overlay Modal",
                    "Dialogs now live in a portal root and are not clipped by parent containers.",
                )
                .with_localized_keys(
                    "showcase-dialog-title",
                    "showcase-dialog-body",
                    "showcase-dialog-close",
                ),
                OverlayConfig {
                    placement: OverlayPlacement::Center,
                    anchor: None,
                    auto_flip: false,
                },
                StyleClass(vec!["i18n.dialog".to_string()]),
            ));
        }
    }

    let combo_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiComboBoxChanged>();

    for event in combo_events {
        if event.action.combo == runtime.locale_combo {
            let next_locale = parse_locale(event.action.value.as_str());
            world
                .resource_mut::<AppI18n>()
                .set_active_locale(next_locale);
            continue;
        }

        if event.action.combo != runtime.combo_box {
            continue;
        }

        world.resource_mut::<ShowcaseState>().selected_combo_value = event.action.value;
    }
}

fn build_i18n_app() -> App {
    init_logging();

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
        cjk_fallback_font_stack(),
    )
    .register_i18n_bundle(
        "zh-CN",
        SyncTextSource::FilePath("assets/locales/zh-CN/main.ftl"),
        zh_cjk_fallback_font_stack(),
    )
    .register_i18n_bundle(
        "ja-JP",
        SyncTextSource::FilePath("assets/locales/ja-JP/main.ftl"),
        ja_cjk_fallback_font_stack(),
    )
    .register_projector::<LocaleBadge>(project_locale_badge)
    .register_projector::<ShowcaseStatus>(project_showcase_status)
    .add_systems(Startup, (setup_i18n_styles, setup_i18n_world))
    .add_systems(PreUpdate, drain_i18n_events);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_i18n_app(), "i18n Showcase", |options| {
        options.with_initial_inner_size(LogicalSize::new(960.0, 520.0))
    })
}
