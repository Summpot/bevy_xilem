//! Bevy + Xilem/Masonry integration with ECS-driven UI projection.
//!
//! `bevy_xilem` lets you:
//! - register projector functions from ECS components to UI views,
//! - collect typed UI actions through [`UiEventQueue`],
//! - synthesize and rebuild a retained Masonry tree every frame.
//!
//! # Minimal setup
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! use bevy_xilem::{
//!     AppBevyXilemExt, BevyXilemPlugin, ProjectionCtx, UiEventQueue, UiRoot, UiView,
//!     bevy_app::{App, PreUpdate, Startup},
//!     bevy_ecs::prelude::*,
//!     text_button,
//! };
//!
//! #[derive(Component, Clone, Copy)]
//! struct Root;
//!
//! #[derive(Debug, Clone, Copy)]
//! enum Action {
//!     Clicked,
//! }
//!
//! fn project_root(_: &Root, ctx: ProjectionCtx<'_>) -> UiView {
//!     Arc::new(text_button(ctx.entity, Action::Clicked, "Click"))
//! }
//!
//! fn setup(mut commands: Commands) {
//!     commands.spawn((UiRoot, Root));
//! }
//!
//! fn drain(world: &mut World) {
//!     let _ = world.resource_mut::<UiEventQueue>().drain_actions::<Action>();
//! }
//!
//! let mut app = App::new();
//! app.add_plugins(BevyXilemPlugin)
//!     .register_projector::<Root>(project_root)
//!     .add_systems(Startup, setup)
//!     .add_systems(PreUpdate, drain);
//! ```
#![forbid(unsafe_code)]

pub mod app_ext;
pub mod ecs;
pub mod events;
pub mod plugin;
pub mod projection;
pub mod runner;
pub mod runtime;
pub mod styling;
pub mod synthesize;
pub mod views;
pub mod widgets;

pub use bevy_app;
pub use bevy_ecs;
pub use bevy_input;
pub use bevy_tasks;
pub use bevy_tweening;
pub use bevy_window;
pub use rfd;
pub use xilem;
pub use xilem_masonry;

pub use app_ext::*;
pub use ecs::*;
pub use events::*;
pub use plugin::*;
pub use projection::*;
pub use runner::*;
pub use runtime::*;
pub use styling::*;
pub use synthesize::*;
pub use views::*;

pub mod prelude {
    //! Convenience exports for building `bevy_xilem` apps.

    pub use bevy_ecs::hierarchy::{ChildOf, Children};

    pub use crate::{
        AppBevyXilemExt, BevyXilemPlugin, BevyXilemRuntime, BuiltinUiAction, ColorStyle,
        ComputedStyle, CurrentColorStyle, EcsButtonView, Hovered, LayoutStyle, MasonryRuntime,
        Pressed, ProjectionCtx, PseudoClass, Selector, StyleClass, StyleDirty, StyleRule,
        StyleSetter, StyleSheet, StyleTransition, SynthesizedUiViews, TargetColorStyle, TextStyle,
        TypedUiEvent, UiAnyView, UiButton, UiEvent, UiEventQueue, UiFlexColumn, UiFlexRow,
        UiInteractionEvent, UiLabel, UiProjector, UiProjectorRegistry, UiRoot, UiSynthesisStats,
        UiView, button, button_with_child, checkbox, ecs_button, ecs_button_with_child,
        ecs_checkbox, ecs_slider, ecs_switch, ecs_text_button, ecs_text_input, emit_ui_action,
        gather_ui_roots, inject_bevy_input_into_masonry, mark_style_dirty, rebuild_masonry_runtime,
        register_builtin_projectors, resolve_style, resolve_style_for_classes,
        resolve_style_for_entity_classes, run_app, run_app_with_window_options, slider, switch,
        synthesize_roots, synthesize_roots_with_stats, synthesize_ui, synthesize_world,
        text_button, text_input, xilem_button, xilem_button_any_pointer, xilem_checkbox,
        xilem_slider, xilem_switch, xilem_text_button, xilem_text_input,
    };

    pub use crate::{
        bevy_app, bevy_ecs, bevy_input, bevy_tasks, bevy_tweening, bevy_window, rfd, xilem,
        xilem_masonry,
    };
}

#[cfg(test)]
mod tests;
