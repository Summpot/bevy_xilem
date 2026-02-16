use bevy_app::{App, Plugin, PostUpdate, PreUpdate, Update};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_input::mouse::{MouseButtonInput, MouseWheel};
use bevy_time::TimePlugin;
use bevy_window::{CursorLeft, CursorMoved, WindowResized};

use crate::{
    events::UiEventQueue,
    projection::{UiProjectorRegistry, register_builtin_projectors},
    runtime::{MasonryRuntime, inject_bevy_input_into_masonry, rebuild_masonry_runtime},
    styling::{
        StyleSheet, animate_style_transitions, sync_style_targets, sync_ui_interaction_markers,
    },
    synthesize::{SynthesizedUiViews, UiSynthesisStats, synthesize_ui},
};

/// Bevy plugin for headless Masonry runtime + ECS projection synthesis.
#[derive(Default)]
pub struct BevyXilemPlugin;

impl Plugin for BevyXilemPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TimePlugin)
            .init_resource::<UiProjectorRegistry>()
            .init_resource::<SynthesizedUiViews>()
            .init_resource::<UiSynthesisStats>()
            .init_resource::<UiEventQueue>()
            .init_resource::<StyleSheet>()
            .init_non_send::<MasonryRuntime>()
            .add_message::<CursorMoved>()
            .add_message::<CursorLeft>()
            .add_message::<MouseButtonInput>()
            .add_message::<MouseWheel>()
            .add_message::<WindowResized>()
            .add_systems(
                PreUpdate,
                (inject_bevy_input_into_masonry, sync_ui_interaction_markers).chain(),
            )
            .add_systems(
                Update,
                (sync_style_targets, animate_style_transitions).chain(),
            )
            .add_systems(PostUpdate, (synthesize_ui, rebuild_masonry_runtime).chain());

        let mut registry = app.world_mut().resource_mut::<UiProjectorRegistry>();
        register_builtin_projectors(&mut registry);
    }
}
