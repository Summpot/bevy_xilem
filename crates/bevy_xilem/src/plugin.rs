use bevy_app::{App, Plugin, PostUpdate, PreUpdate, Update};
use bevy_asset::AssetEvent;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_input::mouse::{MouseButtonInput, MouseWheel};
use bevy_text::Font;
use bevy_time::TimePlugin;
use bevy_tweening::{AnimationSystem, TweeningPlugin};
use bevy_window::{CursorLeft, CursorMoved, WindowResized};

use crate::{
    events::UiEventQueue,
    fonts::{XilemFontBridge, collect_bevy_font_assets, sync_fonts_to_xilem},
    i18n::AppI18n,
    projection::{UiProjectorRegistry, register_builtin_projectors},
    runtime::{MasonryRuntime, inject_bevy_input_into_masonry, rebuild_masonry_runtime},
    styling::{
        StyleSheet, animate_style_transitions, mark_style_dirty, sync_style_targets,
        sync_ui_interaction_markers,
    },
    synthesize::{SynthesizedUiViews, UiSynthesisStats, synthesize_ui},
};

/// Bevy plugin for headless Masonry runtime + ECS projection synthesis.
#[derive(Default)]
pub struct BevyXilemPlugin;

impl Plugin for BevyXilemPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((TimePlugin, TweeningPlugin))
            .init_resource::<UiProjectorRegistry>()
            .init_resource::<SynthesizedUiViews>()
            .init_resource::<UiSynthesisStats>()
            .init_resource::<UiEventQueue>()
            .init_resource::<StyleSheet>()
            .init_resource::<XilemFontBridge>()
            .init_resource::<AppI18n>()
            .init_non_send_resource::<MasonryRuntime>()
            .add_message::<CursorMoved>()
            .add_message::<CursorLeft>()
            .add_message::<MouseButtonInput>()
            .add_message::<MouseWheel>()
            .add_message::<WindowResized>()
            .add_message::<AssetEvent<Font>>()
            .add_systems(
                PreUpdate,
                (
                    collect_bevy_font_assets,
                    sync_fonts_to_xilem,
                    inject_bevy_input_into_masonry,
                    sync_ui_interaction_markers,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (mark_style_dirty, sync_style_targets)
                    .chain()
                    .before(AnimationSystem::AnimationUpdate),
            )
            .add_systems(
                Update,
                animate_style_transitions.after(AnimationSystem::AnimationUpdate),
            )
            .add_systems(PostUpdate, (synthesize_ui, rebuild_masonry_runtime).chain());

        let mut registry = app.world_mut().resource_mut::<UiProjectorRegistry>();
        register_builtin_projectors(&mut registry);
    }
}
