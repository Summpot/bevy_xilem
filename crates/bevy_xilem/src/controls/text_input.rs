use bevy_ecs::{entity::Entity, prelude::*};

use crate::{
    ProjectionCtx, StyleClass, UiLabel, UiView, controls::UiControlTemplate,
    templates::ensure_template_part,
};

/// Built-in text input control with ECS-owned content.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiTextInput {
    pub value: String,
    pub placeholder: String,
}

impl UiTextInput {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            placeholder: String::new(),
        }
    }

    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }
}

/// Emitted when [`UiTextInput`] value changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiTextInputChanged {
    pub input: Entity,
    pub value: String,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartTextInputField;

impl UiControlTemplate for UiTextInput {
    fn expand(world: &mut World, entity: Entity) {
        let placeholder = world
            .get::<UiTextInput>(entity)
            .map(|input| input.placeholder.clone());
        let Some(placeholder) = placeholder else {
            return;
        };

        let field = ensure_template_part::<PartTextInputField, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.text_input.field".to_string()]),
            )
        });

        if let Some(mut label) = world.get_mut::<UiLabel>(field) {
            label.text = placeholder;
        }
    }

    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::elements::project_text_input(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Type("UiTextInput"),
      setter: (
        layout: (
          padding: 8.0,
          corner_radius: 8.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#131C2E"),
          border: Hex("#41557F"),
          text: Hex("#E6EEFF"),
        ),
      ),
    ),
  ],
)
"##
    }
}
