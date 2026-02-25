use bevy_ecs::prelude::*;

use crate::{
    AutoDismiss, OverlayComputedPosition, OverlayConfig, OverlayPlacement, OverlayState,
    ProjectionCtx, UiView, components::UiComponentTemplate,
};

/// Visual severity / colour of a [`UiToast`] notification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ToastKind {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

/// An auto-dismissing toast notification shown in the overlay corner.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct UiToast {
    pub message: String,
    pub kind: ToastKind,
    /// Total display duration in seconds. 0.0 means it persists until manually dismissed.
    pub duration_secs: f32,
    /// Elapsed display time. Updated each frame by the toast tick system.
    pub elapsed_secs: f32,
    /// Preferred overlay placement for this toast.
    pub placement: OverlayPlacement,
    /// Whether toast placement may auto-flip when it would overflow.
    pub auto_flip_placement: bool,
    /// Whether the close button should be rendered.
    pub show_close_button: bool,
    /// Minimum preferred toast width.
    pub min_width: f64,
    /// Maximum preferred toast width.
    pub max_width: f64,
}

impl UiToast {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Info,
            duration_secs: 3.0,
            elapsed_secs: 0.0,
            placement: OverlayPlacement::BottomEnd,
            auto_flip_placement: false,
            show_close_button: true,
            min_width: 220.0,
            max_width: 420.0,
        }
    }

    #[must_use]
    pub fn with_kind(mut self, kind: ToastKind) -> Self {
        self.kind = kind;
        self
    }

    #[must_use]
    pub fn with_duration(mut self, duration_secs: f32) -> Self {
        self.duration_secs = duration_secs;
        self
    }

    #[must_use]
    pub fn with_placement(mut self, placement: OverlayPlacement) -> Self {
        self.placement = placement;
        self
    }

    #[must_use]
    pub fn with_auto_flip_placement(mut self, auto_flip: bool) -> Self {
        self.auto_flip_placement = auto_flip;
        self
    }

    #[must_use]
    pub fn with_show_close_button(mut self, show: bool) -> Self {
        self.show_close_button = show;
        self
    }

    #[must_use]
    pub fn with_min_width(mut self, width: f64) -> Self {
        self.min_width = width.max(0.0);
        self
    }

    #[must_use]
    pub fn with_max_width(mut self, width: f64) -> Self {
        self.max_width = width.max(0.0);
        self
    }
}

impl UiComponentTemplate for UiToast {
    fn expand(world: &mut World, entity: Entity) {
        let toast = world.get::<UiToast>(entity).cloned();
        let Some(toast) = toast else {
            return;
        };

        if world.get::<OverlayConfig>(entity).is_none() {
            world.entity_mut(entity).insert(OverlayConfig {
                placement: toast.placement,
                anchor: None,
                auto_flip: toast.auto_flip_placement,
            });
        }

        if world.get::<OverlayState>(entity).is_none() {
            world.entity_mut(entity).insert(OverlayState {
                is_modal: false,
                anchor: None,
            });
        }

        if world.get::<OverlayComputedPosition>(entity).is_none() {
            world
                .entity_mut(entity)
                .insert(OverlayComputedPosition::default());
        }

        if toast.duration_secs > 0.0 {
            if world.get::<AutoDismiss>(entity).is_none() {
                world
                    .entity_mut(entity)
                    .insert(AutoDismiss::from_seconds(toast.duration_secs));
            }
        } else if world.get::<AutoDismiss>(entity).is_some() {
            world.entity_mut(entity).remove::<AutoDismiss>();
        }
    }

    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_toast(component, ctx)
    }
}
