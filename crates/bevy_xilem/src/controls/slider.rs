use bevy_ecs::{entity::Entity, prelude::*};

use crate::{
    ProjectionCtx, StyleClass, UiLabel, UiView, controls::UiControlTemplate,
    templates::ensure_template_part,
};

/// Built-in slider control with ECS-native value.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct UiSlider {
    pub min: f64,
    pub max: f64,
    pub value: f64,
    /// Default step used by built-in increment/decrement actions.
    pub step: f64,
}

impl UiSlider {
    #[must_use]
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        let min = min.min(max);
        let max = max.max(min);
        let value = value.clamp(min, max);
        let span = (max - min).abs();
        let step = (span / 20.0).max(0.01);
        Self {
            min,
            max,
            value,
            step,
        }
    }

    #[must_use]
    pub fn with_step(mut self, step: f64) -> Self {
        self.step = step.abs().max(f64::EPSILON);
        self
    }
}

/// Emitted when [`UiSlider`] value changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiSliderChanged {
    pub slider: Entity,
    pub value: f64,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartSliderDecrease;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartSliderTrack;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartSliderThumb;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartSliderIncrease;

impl UiControlTemplate for UiSlider {
    fn expand(world: &mut World, entity: Entity) {
        let value = world.get::<UiSlider>(entity).map(|slider| slider.value);
        let Some(value) = value else {
            return;
        };

        let dec = ensure_template_part::<PartSliderDecrease, _>(world, entity, || {
            (
                UiLabel::new("−"),
                StyleClass(vec!["template.slider.decrease".to_string()]),
            )
        });
        let track = ensure_template_part::<PartSliderTrack, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.slider.track".to_string()]),
            )
        });
        let thumb = ensure_template_part::<PartSliderThumb, _>(world, entity, || {
            (
                UiLabel::new("●"),
                StyleClass(vec!["template.slider.thumb".to_string()]),
            )
        });
        let inc = ensure_template_part::<PartSliderIncrease, _>(world, entity, || {
            (
                UiLabel::new("+"),
                StyleClass(vec!["template.slider.increase".to_string()]),
            )
        });

        if let Some(mut label) = world.get_mut::<UiLabel>(dec) {
            label.text = "−".to_string();
        }
        if let Some(mut label) = world.get_mut::<UiLabel>(track) {
            label.text = format!("{value:.2}");
        }
        if let Some(mut label) = world.get_mut::<UiLabel>(thumb) {
            label.text = "●".to_string();
        }
        if let Some(mut label) = world.get_mut::<UiLabel>(inc) {
            label.text = "+".to_string();
        }
    }

    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::elements::project_slider(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Type("UiSlider"),
      setter: (
        layout: (
          padding: 6.0,
          corner_radius: 6.0,
          border_width: 1.0,
          gap: 8.0,
        ),
        colors: (
          bg: Hex("#161F33"),
          border: Hex("#41557F"),
          text: Hex("#DCE7FF"),
        ),
      ),
    ),
    (
      selector: Class("template.slider.track"),
      setter: (
        colors: (
          text: Hex("#BFD4FF"),
        ),
      ),
    ),
    (
      selector: Class("template.slider.thumb"),
      setter: (
        colors: (
          text: Hex("#60A5FA"),
        ),
      ),
    ),
  ],
)
"##
    }
}
