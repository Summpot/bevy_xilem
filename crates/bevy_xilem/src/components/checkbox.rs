use bevy_ecs::{entity::Entity, prelude::*};

use crate::{
    ProjectionCtx, StyleClass, UiLabel, UiView, components::UiComponentTemplate,
    templates::ensure_template_part,
};

/// Built-in checkbox UI component with ECS-native state.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiCheckbox {
    pub label: String,
    pub checked: bool,
}

impl UiCheckbox {
    #[must_use]
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
        }
    }
}

/// Emitted when [`UiCheckbox`] state changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiCheckboxChanged {
    pub checkbox: Entity,
    pub checked: bool,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartCheckboxIndicator;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartCheckboxLabel;

impl UiComponentTemplate for UiCheckbox {
    fn expand(world: &mut World, entity: Entity) {
        let checkbox = world.get::<UiCheckbox>(entity).cloned();
        let Some(checkbox) = checkbox else {
            return;
        };

        let indicator = ensure_template_part::<PartCheckboxIndicator, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.checkbox.indicator".to_string()]),
            )
        });
        let label_part = ensure_template_part::<PartCheckboxLabel, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.checkbox.label".to_string()]),
            )
        });

        if let Some(mut label) = world.get_mut::<UiLabel>(indicator) {
            label.text = if checkbox.checked {
                "☑".to_string()
            } else {
                "☐".to_string()
            };
        }
        if let Some(mut label) = world.get_mut::<UiLabel>(label_part) {
            label.text = checkbox.label;
        }
    }

    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::elements::project_checkbox(component, ctx)
    }
}
