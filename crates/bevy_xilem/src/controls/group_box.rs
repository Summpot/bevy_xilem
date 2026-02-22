use bevy_ecs::prelude::*;

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

/// A titled group box that visually groups related content.
///
/// Place content entities as ECS children.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiGroupBox {
    /// Title displayed at the top of the group box.
    pub title: String,
}

impl UiGroupBox {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

impl UiControlTemplate for UiGroupBox {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_group_box(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Class("widget.group_box.title"),
      setter: (
        text: (
          size: 15.0,
        ),
        colors: (
          text: Hex("#E7EEFF"),
        ),
      ),
    ),
  ],
)
"##
    }
}
