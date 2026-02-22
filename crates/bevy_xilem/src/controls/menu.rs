use bevy_ecs::{entity::Entity, prelude::*};

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

/// A single item in a menu (inside a dropdown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiMenuItem {
    pub label: String,
    pub value: String,
}

impl UiMenuItem {
    #[must_use]
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

/// A top-level entry in a menu bar with a dropdown list of menu items.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiMenuBarItem {
    /// Label displayed on the menu bar button.
    pub label: String,
    /// Items shown in the dropdown panel.
    pub items: Vec<UiMenuItem>,
    /// Whether the dropdown is currently open.
    pub is_open: bool,
}

impl UiMenuBarItem {
    #[must_use]
    pub fn new(label: impl Into<String>, items: impl IntoIterator<Item = UiMenuItem>) -> Self {
        Self {
            label: label.into(),
            items: items.into_iter().collect(),
            is_open: false,
        }
    }
}

/// Marker for a horizontal menu bar container.
///
/// Place [`UiMenuBarItem`] entities as ECS children.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiMenuBar;

/// Floating menu item panel rendered in the overlay layer (one per open [`UiMenuBarItem`]).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiMenuItemPanel {
    /// The [`UiMenuBarItem`] anchor entity this panel belongs to.
    pub anchor: Entity,
}

/// Emitted when a menu item is selected from a [`UiMenuBarItem`] dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiMenuItemSelected {
    pub bar_item: Entity,
    pub value: String,
}

impl UiControlTemplate for UiMenuBar {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_menu_bar(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Type("UiMenuBar"),
      setter: (
        layout: (
          padding: 4.0,
          gap: 4.0,
          corner_radius: 6.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#232323"),
          border: Hex("#3F3F3F"),
        ),
      ),
    ),
  ],
)
"##
    }
}

impl UiControlTemplate for UiMenuBarItem {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_menu_bar_item(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Type("UiMenuBarItem"),
      setter: (
        layout: (
          padding: 6.0,
          corner_radius: 4.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#272727"),
          hover_bg: Hex("#323232"),
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

impl UiControlTemplate for UiMenuItemPanel {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_menu_item_panel(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Class("overlay.menu.panel"),
      setter: (
        layout: (
          padding: 8.0,
          corner_radius: 6.0,
          border_width: 1.0,
          gap: 4.0,
        ),
        colors: (
          bg: Hex("#1F1F1F"),
          border: Hex("#3F3F3F"),
        ),
      ),
    ),
    (
      selector: Class("overlay.menu.item"),
      setter: (
        layout: (
          padding: 6.0,
          corner_radius: 4.0,
          border_width: 1.0,
        ),
        colors: (
          text: Hex("#F3F3F3"),
          hover_bg: Hex("#323232"),
          pressed_bg: Hex("#272727"),
          border: Hex("#3F3F3F"),
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
