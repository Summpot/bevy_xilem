use bevy_ecs::{entity::Entity, prelude::*};

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

/// Radio button group component with multiple exclusive options.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiRadioGroup {
    /// Labels for each radio option.
    pub options: Vec<String>,
    /// Index of the currently selected option.
    pub selected: usize,
}

impl UiRadioGroup {
    #[must_use]
    pub fn new(options: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            options: options.into_iter().map(Into::into).collect(),
            selected: 0,
        }
    }

    #[must_use]
    pub fn with_selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

/// Emitted when the selection in a [`UiRadioGroup`] changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiRadioGroupChanged {
    pub group: Entity,
    pub selected: usize,
}

impl UiControlTemplate for UiRadioGroup {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_radio_group(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Class("widget.radio.item"),
      setter: (
        layout: (
          padding: 4.0,
        ),
      ),
    ),
  ],
)
"##
    }
}
