use bevy_ecs::{entity::Entity, prelude::*};

use crate::{ProjectionCtx, UiView, components::UiComponentTemplate};

/// Causes a floating tooltip to appear when the entity is hovered.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct HasTooltip {
    /// Text shown inside the tooltip.
    pub text: String,
}

impl HasTooltip {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Floating tooltip overlay anchored to a source entity.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiTooltip {
    /// Tooltip body text.
    pub text: String,
    /// The entity that triggered this tooltip.
    pub anchor: Entity,
}

impl UiComponentTemplate for UiTooltip {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_tooltip(component, ctx)
    }
}
