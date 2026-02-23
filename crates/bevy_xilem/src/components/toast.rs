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
}

impl UiToast {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Info,
            duration_secs: 3.0,
            elapsed_secs: 0.0,
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
}

impl UiComponentTemplate for UiToast {
    fn expand(world: &mut World, entity: Entity) {
        let toast = world.get::<UiToast>(entity).cloned();
        let Some(toast) = toast else {
            return;
        };

        if world.get::<OverlayConfig>(entity).is_none() {
            world.entity_mut(entity).insert(OverlayConfig {
                placement: OverlayPlacement::Bottom,
                anchor: None,
                auto_flip: false,
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
