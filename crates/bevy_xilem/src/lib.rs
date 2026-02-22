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
pub mod controls;
pub mod ecs;
pub mod events;
pub mod fonts;
pub mod i18n;
pub mod overlay;
pub mod plugin;
pub mod projection;
pub mod runner;
pub mod runtime;
pub mod styling;
pub mod synthesize;
pub mod templates;
pub mod views;
pub mod widget_actions;
pub mod widgets;

pub use bevy_app;
pub use bevy_asset;
pub use bevy_ecs;
pub use bevy_input;
pub use bevy_math;
pub use bevy_tasks;
pub use bevy_text;
pub use bevy_tweening;
pub use bevy_window;
pub use rfd;
pub use xilem;
pub use xilem_masonry;

pub use app_ext::*;
pub use controls::*;
pub use ecs::*;
pub use events::*;
pub use fonts::*;
pub use i18n::*;
pub use overlay::*;
pub use plugin::*;
pub use projection::*;
pub use runner::*;
pub use runtime::*;
pub use styling::*;
pub use synthesize::*;
pub use templates::*;
pub use views::*;
pub use widget_actions::*;

pub mod prelude {
    //! Convenience exports for building `bevy_xilem` apps.

    pub use bevy_ecs::hierarchy::{ChildOf, Children};

    pub use crate::{
        AppBevyXilemExt, AppI18n, AutoDismiss, BevyWindowOptions, BevyXilemPlugin, BuiltinUiAction,
        ColorStyle, ComputedStyle, CurrentColorStyle, EcsButtonView, HasTooltip, Hovered,
        LayoutStyle, LocalizeText, MasonryRuntime, OverlayComputedPosition, OverlayConfig,
        OverlayMouseButtonCursor, OverlayPlacement, OverlayPointerRoutingState, OverlayStack,
        OverlayState, OverlayUiAction, Pressed, ProjectionCtx, PseudoClass, Selector,
        SplitDirection, StopUiPointerPropagation, StyleClass, StyleDirty, StyleRule, StyleSetter,
        StyleSheet, StyleTransition, SyncAssetSource, SyncTextSource, SynthesizedUiViews,
        TargetColorStyle, TextStyle, ToastKind, TypedUiEvent, UiAnyView, UiButton, UiCheckbox,
        UiCheckboxChanged, UiColorPicker, UiColorPickerChanged, UiColorPickerPanel, UiComboBox,
        UiComboBoxChanged, UiComboOption, UiControlTemplate, UiDatePicker, UiDatePickerChanged,
        UiDatePickerPanel, UiDialog, UiDropdownMenu, UiDropdownPlacement, UiEvent, UiEventQueue,
        UiFlexColumn, UiFlexRow, UiGroupBox, UiInteractionEvent, UiLabel, UiMenuBar, UiMenuBarItem,
        UiMenuItem, UiMenuItemPanel, UiMenuItemSelected, UiOverlayRoot, UiPointerEvent,
        UiPointerHitEvent, UiPointerPhase, UiProjector, UiProjectorRegistry, UiRadioGroup,
        UiRadioGroupChanged, UiRoot, UiSlider, UiSliderChanged, UiSpinner, UiSplitPane, UiSwitch,
        UiSwitchChanged, UiSynthesisStats, UiTabBar, UiTabChanged, UiTable, UiTextInput,
        UiTextInputChanged, UiToast, UiTooltip, UiTreeNode, UiTreeNodeToggled, UiView,
        WidgetUiAction, XilemFontBridge, bubble_ui_pointer_events, button, button_with_child,
        checkbox, collect_bevy_font_assets, dismiss_overlays_on_click, ecs_button,
        ecs_button_with_child, ecs_checkbox, ecs_slider, ecs_switch, ecs_text_button,
        ecs_text_input, emit_ui_action, ensure_overlay_root, ensure_overlay_root_entity,
        ensure_template_part, expand_builtin_control_templates, find_template_part,
        gather_ui_roots, handle_global_overlay_clicks, handle_overlay_actions,
        handle_tooltip_hovers, handle_widget_actions, inject_bevy_input_into_masonry,
        mark_style_dirty, rebuild_masonry_runtime, register_builtin_projectors,
        register_builtin_style_type_aliases, register_builtin_ui_controls, resolve_localized_text,
        resolve_style, resolve_style_for_classes, resolve_style_for_entity_classes, run_app,
        run_app_with_window_options, slider, spawn_in_overlay_root, switch,
        sync_dropdown_positions, sync_fonts_to_xilem, sync_overlay_positions,
        sync_overlay_stack_lifecycle, synthesize_roots, synthesize_roots_with_stats, synthesize_ui,
        synthesize_world, text_button, text_input, tick_toasts, xilem_button,
        xilem_button_any_pointer, xilem_checkbox, xilem_slider, xilem_switch, xilem_text_button,
        xilem_text_input,
    };

    pub use crate::{
        bevy_app, bevy_asset, bevy_ecs, bevy_input, bevy_math, bevy_tasks, bevy_text,
        bevy_tweening, bevy_window, rfd, xilem, xilem_masonry,
    };
}

#[cfg(test)]
mod tests;
