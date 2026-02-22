use bevy_ecs::{entity::Entity, prelude::*};

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

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

impl UiControlTemplate for UiTooltip {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_tooltip(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Class("overlay.tooltip"),
      setter: (
        layout: (
          padding: 6.0,
          corner_radius: 4.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#0F172A"),
          border: Hex("#24314A"),
          text: Hex("#E2E8F0"),
        ),
      ),
    ),
  ],
)
"##
    }
}
