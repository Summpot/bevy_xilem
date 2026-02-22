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
          padding: 6.0,
          corner_radius: 6.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#272727"),
          hover_bg: Hex("#313131"),
          pressed_bg: Hex("#1F1F1F"),
          border: Hex("#3F3F3F"),
          text: Hex("#F3F3F3"),
        ),
        transition: (
          duration: 0.10,
        ),
      ),
    ),
  ],
)
"##
    }
}
