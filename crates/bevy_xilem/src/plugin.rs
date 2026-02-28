use bevy_app::{App, Last, Plugin, PostUpdate, PreUpdate, TaskPoolPlugin, Update};
use bevy_asset::{AssetApp, AssetEvent, AssetPlugin};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_input::keyboard::KeyboardInput;
use bevy_input::mouse::{MouseButtonInput, MouseWheel};
use bevy_text::Font;
use bevy_time::TimePlugin;
use bevy_tweening::{AnimationSystem, TweeningPlugin};
use bevy_window::{
    CursorLeft, CursorMoved, Ime, WindowFocused, WindowResized, WindowScaleFactorChanged,
};

use crate::{
    AppBevyXilemExt, OverlayStack,
    components::register_builtin_ui_components,
    events::UiEventQueue,
    fonts::{XilemFontBridge, collect_bevy_font_assets, sync_fonts_to_xilem},
    i18n::AppI18n,
    overlay::{
        OverlayPointerRoutingState, bubble_ui_pointer_events, ensure_overlay_defaults,
        ensure_overlay_root, handle_global_overlay_clicks, handle_overlay_actions,
        reparent_overlay_entities, sync_overlay_positions, sync_overlay_stack_lifecycle,
    },
    projection::{UiProjectorRegistry, register_core_projectors},
    runtime::{
        MasonryRuntime, initialize_masonry_runtime_from_primary_window,
        inject_bevy_input_into_masonry, paint_masonry_ui, rebuild_masonry_runtime,
    },
    styling::{
        ActiveStyleSheet, ActiveStyleSheetAsset, ActiveStyleSheetSelectors,
        ActiveStyleSheetTokenNames, BaseStyleSheet, RegisteredStyleVariants, StyleAssetEventCursor,
        StyleSheet, StyleSheetRonLoader, animate_style_transitions,
        ensure_active_stylesheet_asset_handle, install_embedded_fluent_theme_default_variant,
        mark_style_dirty, register_builtin_style_type_aliases, sync_style_targets,
        sync_stylesheet_asset_events, sync_ui_interaction_markers,
    },
    synthesize::{SynthesizedUiViews, UiSynthesisStats, synthesize_ui},
    widget_actions::{
        handle_scroll_view_wheel, handle_tooltip_hovers, handle_widget_actions,
        sync_scroll_view_layout_geometry, tick_auto_dismiss,
    },
};

/// Bevy plugin for headless Masonry runtime + ECS projection synthesis.
#[derive(Default)]
pub struct BevyXilemPlugin;

/// Registers all built-in ECS UI components.
///
/// This plugin is automatically added by [`BevyXilemPlugin`], so users get
/// plug-and-play built-ins without manual registration in app setup code.
#[derive(Default)]
pub struct BevyXilemBuiltinsPlugin;

impl Plugin for BevyXilemBuiltinsPlugin {
    fn build(&self, app: &mut App) {
        register_builtin_ui_components(app);
    }
}

impl Plugin for BevyXilemPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TaskPoolPlugin>() {
            app.add_plugins(TaskPoolPlugin::default());
        }
        if !app.is_plugin_added::<AssetPlugin>() {
            app.add_plugins(AssetPlugin::default());
        }

        app.add_plugins((TimePlugin, TweeningPlugin, BevyXilemBuiltinsPlugin))
            .register_xilem_font_bytes(crate::icons::LUCIDE_FONT_BYTES)
            .init_asset::<StyleSheet>()
            .init_asset_loader::<StyleSheetRonLoader>()
            .init_resource::<UiProjectorRegistry>()
            .init_resource::<SynthesizedUiViews>()
            .init_resource::<UiSynthesisStats>()
            .init_resource::<UiEventQueue>()
            .init_resource::<StyleSheet>()
            .init_resource::<BaseStyleSheet>()
            .init_resource::<ActiveStyleSheet>()
            .init_resource::<ActiveStyleSheetAsset>()
            .init_resource::<ActiveStyleSheetSelectors>()
            .init_resource::<ActiveStyleSheetTokenNames>()
            .init_resource::<RegisteredStyleVariants>()
            .init_resource::<StyleAssetEventCursor>()
            .init_resource::<XilemFontBridge>()
            .init_resource::<AppI18n>()
            .init_resource::<OverlayStack>()
            .init_resource::<OverlayPointerRoutingState>()
            .init_non_send_resource::<MasonryRuntime>()
            .add_message::<CursorMoved>()
            .add_message::<CursorLeft>()
            .add_message::<KeyboardInput>()
            .add_message::<MouseButtonInput>()
            .add_message::<MouseWheel>()
            .add_message::<Ime>()
            .add_message::<WindowFocused>()
            .add_message::<WindowResized>()
            .add_message::<WindowScaleFactorChanged>()
            .add_message::<AssetEvent<Font>>()
            .add_systems(
                PreUpdate,
                (
                    collect_bevy_font_assets,
                    sync_fonts_to_xilem,
                    initialize_masonry_runtime_from_primary_window,
                    bubble_ui_pointer_events,
                    handle_global_overlay_clicks,
                    sync_scroll_view_layout_geometry,
                    handle_scroll_view_wheel,
                    inject_bevy_input_into_masonry,
                    sync_ui_interaction_markers,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    ensure_overlay_root,
                    reparent_overlay_entities,
                    ensure_overlay_defaults,
                    handle_overlay_actions,
                    handle_widget_actions,
                    handle_tooltip_hovers,
                    tick_auto_dismiss,
                    sync_overlay_stack_lifecycle,
                    ensure_active_stylesheet_asset_handle,
                    sync_stylesheet_asset_events,
                    mark_style_dirty,
                    sync_style_targets,
                )
                    .chain()
                    .before(AnimationSystem::AnimationUpdate),
            )
            .add_systems(
                Update,
                animate_style_transitions.after(AnimationSystem::AnimationUpdate),
            )
            .add_systems(PostUpdate, (synthesize_ui, rebuild_masonry_runtime).chain());

        // Run overlay placement after Masonry's retained tree has been rebuilt,
        // so anchor/widget geometry is up-to-date for this frame.
        app.add_systems(
            PostUpdate,
            sync_overlay_positions.after(rebuild_masonry_runtime),
        );

        app.add_systems(Last, paint_masonry_ui);

        register_builtin_style_type_aliases(app.world_mut());
        install_embedded_fluent_theme_default_variant(app.world_mut()).unwrap_or_else(|error| {
            panic!("failed to parse embedded Fluent theme bundle: {error}")
        });

        {
            let mut registry = app.world_mut().resource_mut::<UiProjectorRegistry>();
            register_core_projectors(&mut registry);
        }
    }
}
