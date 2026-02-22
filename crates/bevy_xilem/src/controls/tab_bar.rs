use bevy_ecs::{entity::Entity, prelude::*};

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

/// Tab bar component that shows labeled tabs and manages active content.
///
/// Place tab content entities as ECS children; the active tab index
/// controls which child is displayed.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiTabBar {
    /// Labels shown on each tab header.
    pub tabs: Vec<String>,
    /// Index of the currently active tab.
    pub active: usize,
}

impl UiTabBar {
    #[must_use]
    pub fn new(tabs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            tabs: tabs.into_iter().map(Into::into).collect(),
            active: 0,
        }
    }

    #[must_use]
    pub fn with_active(mut self, index: usize) -> Self {
        self.active = index;
        self
    }
}

/// Emitted when the active tab changes in a [`UiTabBar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiTabChanged {
    pub bar: Entity,
    pub active: usize,
}

impl UiControlTemplate for UiTabBar {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_tab_bar(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Class("widget.tab.header"),
      setter: (
        layout: (
          padding: 8.0,
          corner_radius: 6.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#1B263D"),
          border: Hex("#41557F"),
          text: Hex("#DCE7FF"),
        ),
      ),
    ),
    (
      selector: Class("widget.tab.active"),
      setter: (
        layout: (
          padding: 8.0,
          corner_radius: 6.0,
          border_width: 2.0,
        ),
        colors: (
          bg: Hex("#243655"),
          border: Hex("#60A5FA"),
          text: Hex("#EFF5FF"),
        ),
      ),
    ),
  ],
)
"##
    }
}
