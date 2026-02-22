use bevy_ecs::prelude::*;

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

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

impl UiControlTemplate for UiToast {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_toast(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
    rules: [
        (
            selector: Type("UiToast"),
            setter: (
                layout: (
                    padding: 8.0,
                    corner_radius: 6.0,
                    border_width: 1.0,
                    gap: 8.0,
                ),
                colors: (
                    bg: Hex("#272727"),
                    border: Hex("#3F3F3F"),
                    text: Hex("#F3F3F3"),
                ),
            ),
        ),
    ],
)
"##
    }
}
