use bevy_ecs::prelude::*;

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

/// An animated loading spinner (indefinite progress indicator).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiSpinner {
    /// Optional label shown next to the spinner.
    pub label: Option<String>,
}

impl UiSpinner {
    #[must_use]
    pub fn new() -> Self {
        Self { label: None }
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Default for UiSpinner {
    fn default() -> Self {
        Self::new()
    }
}

impl UiControlTemplate for UiSpinner {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_spinner(component, ctx)
    }
}
