use bevy_ecs::prelude::*;

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

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

impl UiControlTemplate for UiButton {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::elements::project_button(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Type("UiButton"),
      setter: (
        layout: (
          padding: 8.0,
          corner_radius: 8.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#25334F"),
          hover_bg: Hex("#2E3E5F"),
          pressed_bg: Hex("#1D2B44"),
          border: Hex("#4F6695"),
          text: Hex("#DFE9FF"),
        ),
        transition: (
          duration: 0.12,
        ),
      ),
    ),
  ],
)
"##
    }
}
