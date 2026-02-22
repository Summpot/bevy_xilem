use bevy_ecs::{entity::Entity, prelude::*};

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

/// An inline color picker that opens an overlay panel for color selection.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiColorPicker {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// Whether the color picker overlay panel is currently open.
    pub is_open: bool,
}

impl UiColorPicker {
    #[must_use]
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self {
            r,
            g,
            b,
            is_open: false,
        }
    }
}

/// Floating color picker panel (rendered in the overlay layer).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiColorPickerPanel {
    /// The [`UiColorPicker`] anchor entity this panel belongs to.
    pub anchor: Entity,
}

/// Emitted when the selected color changes in a [`UiColorPicker`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiColorPickerChanged {
    pub picker: Entity,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl UiControlTemplate for UiColorPicker {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_color_picker(component, ctx)
    }
}

impl UiControlTemplate for UiColorPickerPanel {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_color_picker_panel(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Class("overlay.color_picker.panel"),
      setter: (
        layout: (
          padding: 8.0,
          corner_radius: 8.0,
          border_width: 1.0,
          gap: 6.0,
        ),
        colors: (
          bg: Hex("#161C2A"),
          border: Hex("#384664"),
        ),
      ),
    ),
    (
      selector: Class("overlay.color_picker.swatch"),
      setter: (
        layout: (
          corner_radius: 3.0,
          border_width: 1.0,
        ),
        colors: (
          border: Rgba8(255, 255, 255, 80),
        ),
      ),
    ),
    (
      selector: Class("overlay.color_picker.value"),
      setter: (
        colors: (
          text: Hex("#DCE7FF"),
        ),
      ),
    ),
  ],
)
"##
    }
}
