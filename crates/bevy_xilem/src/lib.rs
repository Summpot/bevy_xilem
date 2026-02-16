#![forbid(unsafe_code)]

pub mod ecs;
pub mod events;
pub mod plugin;
pub mod projection;
pub mod runner;
pub mod runtime;
pub mod synthesize;
pub mod views;
pub mod widgets;

pub use ecs::*;
pub use events::*;
pub use plugin::*;
pub use projection::*;
pub use runner::*;
pub use runtime::*;
pub use synthesize::*;
pub use views::*;

pub mod prelude {
    pub use bevy_ecs::hierarchy::{ChildOf, Children};

    pub use crate::{
        BevyXilemPlugin, BevyXilemRuntime, BuiltinUiAction, EcsButtonView, MasonryRuntime,
        ProjectionCtx, SynthesizedUiViews, TypedUiEvent, UiAnyView, UiButton, UiEvent,
        UiEventQueue, UiFlexColumn, UiFlexRow, UiLabel, UiNodeId, UiProjector, UiProjectorRegistry,
        UiRoot, UiSynthesisStats, UiView, ecs_button, ecs_button_with_child, ecs_checkbox,
        ecs_slider, ecs_switch, ecs_text_button, ecs_text_input, emit_ui_action, gather_ui_roots,
        inject_bevy_input_into_masonry, rebuild_masonry_runtime, register_builtin_projectors,
        run_app, run_app_with_window_options, synthesize_roots, synthesize_roots_with_stats,
        synthesize_ui, synthesize_world,
    };
}

#[cfg(test)]
mod tests;
