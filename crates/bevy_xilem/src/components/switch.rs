use bevy_ecs::{entity::Entity, prelude::*};

use crate::{
    ProjectionCtx, StyleClass, UiLabel, UiView, components::UiComponentTemplate,
    templates::ensure_template_part,
};

/// Built-in switch/toggle UI component.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiSwitch {
    pub on: bool,
    pub label: Option<String>,
}

impl UiSwitch {
    #[must_use]
    pub fn new(on: bool) -> Self {
        Self { on, label: None }
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// Emitted when [`UiSwitch`] state changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiSwitchChanged {
    pub switch: Entity,
    pub on: bool,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartSwitchTrack;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartSwitchThumb;

impl UiComponentTemplate for UiSwitch {
    fn expand(world: &mut World, entity: Entity) {
        let switch = world.get::<UiSwitch>(entity).cloned();
        let Some(switch) = switch else {
            return;
        };

        let track = ensure_template_part::<PartSwitchTrack, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.switch.track".to_string()]),
            )
        });
        let thumb = ensure_template_part::<PartSwitchThumb, _>(world, entity, || {
            (
                UiLabel::new("●"),
                StyleClass(vec!["template.switch.thumb".to_string()]),
            )
        });

        let state_text = if switch.on { "On" } else { "Off" };
        let full_text = match switch.label {
            Some(label) if !label.is_empty() => format!("{state_text} · {label}"),
            _ => state_text.to_string(),
        };

        if let Some(mut label) = world.get_mut::<UiLabel>(track) {
            label.text = full_text;
        }
        if let Some(mut label) = world.get_mut::<UiLabel>(thumb) {
            label.text = "●".to_string();
        }
    }

    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::elements::project_switch(component, ctx)
    }
}
