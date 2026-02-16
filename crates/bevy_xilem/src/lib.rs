#![forbid(unsafe_code)]

pub mod app_ext;
pub mod ecs;
pub mod events;
pub mod plugin;
pub mod projection;
pub mod runner;
pub mod runtime;
pub mod synthesize;
pub mod views;
pub mod widgets;

pub use bevy_app;
pub use bevy_ecs;
pub use bevy_input;
pub use bevy_window;
pub use xilem;
pub use xilem_masonry;

pub use app_ext::*;
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
        AppBevyXilemExt, BevyXilemPlugin, BevyXilemRuntime, BuiltinUiAction, EcsButtonView,
        MasonryRuntime, ProjectionCtx, SynthesizedUiViews, TypedUiEvent, UiAnyView, UiButton,
        UiEvent, UiEventQueue, UiFlexColumn, UiFlexRow, UiLabel, UiProjector, UiProjectorRegistry,
        UiRoot, UiSynthesisStats, UiView, button, button_with_child, checkbox, ecs_button,
        ecs_button_with_child, ecs_checkbox, ecs_slider, ecs_switch, ecs_text_button,
        ecs_text_input, emit_ui_action, gather_ui_roots, inject_bevy_input_into_masonry,
        rebuild_masonry_runtime, register_builtin_projectors, run_app, run_app_with_window_options,
        slider, switch, synthesize_roots, synthesize_roots_with_stats, synthesize_ui,
        synthesize_world, text_button, text_input, xilem_button, xilem_button_any_pointer,
        xilem_checkbox, xilem_slider, xilem_switch, xilem_text_button, xilem_text_input,
    };

    pub use crate::{bevy_app, bevy_ecs, bevy_input, bevy_window, xilem, xilem_masonry};
}

#[cfg(test)]
mod tests;
