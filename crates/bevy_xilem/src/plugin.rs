use bevy_app::{App, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_input::mouse::{MouseButtonInput, MouseWheel};
use bevy_window::{CursorLeft, CursorMoved, WindowResized};

use crate::{
    events::UiEventQueue,
    projection::{UiProjectorRegistry, register_builtin_projectors},
    runtime::{MasonryRuntime, inject_bevy_input_into_masonry, rebuild_masonry_runtime},
    synthesize::{SynthesizedUiViews, UiSynthesisStats, synthesize_ui},
};

/// Bevy plugin for headless Masonry runtime + ECS projection synthesis.
#[derive(Default)]
pub struct BevyXilemPlugin;

impl Plugin for BevyXilemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiProjectorRegistry>()
            .init_resource::<SynthesizedUiViews>()
            .init_resource::<UiSynthesisStats>()
            .init_resource::<UiEventQueue>()
            .init_non_send::<MasonryRuntime>()
            .add_message::<CursorMoved>()
            .add_message::<CursorLeft>()
            .add_message::<MouseButtonInput>()
            .add_message::<MouseWheel>()
            .add_message::<WindowResized>()
            .add_systems(PreUpdate, inject_bevy_input_into_masonry)
            .add_systems(PostUpdate, (synthesize_ui, rebuild_masonry_runtime).chain());

        let mut registry = app.world_mut().resource_mut::<UiProjectorRegistry>();
        register_builtin_projectors(&mut registry);
    }
}
