use bevy_ecs::{entity::Entity, prelude::*};

use crate::{ProjectionCtx, UiView, components::UiComponentTemplate};

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

impl UiComponentTemplate for UiColorPicker {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_color_picker(component, ctx)
    }
}

impl UiComponentTemplate for UiColorPickerPanel {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_color_picker_panel(component, ctx)
    }
}
