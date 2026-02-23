use bevy_ecs::{entity::Entity, prelude::*};

use crate::{ProjectionCtx, UiView, components::UiComponentTemplate};

/// An inline date picker that opens a calendar overlay panel.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiDatePicker {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// Whether the calendar overlay panel is currently open.
    pub is_open: bool,
}

impl UiDatePicker {
    #[must_use]
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self {
            year,
            month: month.clamp(1, 12),
            day: day.clamp(1, 31),
            is_open: false,
        }
    }
}

/// Floating date picker calendar panel (rendered in the overlay layer).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiDatePickerPanel {
    /// The [`UiDatePicker`] anchor entity this panel belongs to.
    pub anchor: Entity,
    /// Month currently shown in the calendar (may differ from selected month).
    pub view_year: i32,
    pub view_month: u32,
}

/// Emitted when the selected date changes in a [`UiDatePicker`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiDatePickerChanged {
    pub picker: Entity,
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl UiComponentTemplate for UiDatePicker {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_date_picker(component, ctx)
    }
}

impl UiComponentTemplate for UiDatePickerPanel {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_date_picker_panel(component, ctx)
    }
}
