use bevy_ecs::prelude::*;

use crate::{ProjectionCtx, UiView, components::UiComponentTemplate};

/// Built-in button component.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiButton {
    pub label: String,
}

impl UiButton {
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl UiComponentTemplate for UiButton {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::elements::project_button(component, ctx)
    }
}
