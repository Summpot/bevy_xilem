use bevy_ecs::{
    entity::Entity,
    hierarchy::{ChildOf, Children},
    prelude::*,
};

use crate::{
    ecs::{
        PartCheckboxIndicator, PartCheckboxLabel, PartComboBoxChevron, PartComboBoxDisplay,
        PartDialogBody, PartDialogDismiss, PartDialogTitle, PartSliderDecrease, PartSliderIncrease,
        PartSliderThumb, PartSliderTrack, PartSwitchThumb, PartSwitchTrack, PartTextInputField,
        UiCheckbox, UiComboBox, UiDialog, UiLabel, UiSlider, UiSwitch, UiTextInput,
    },
    styling::StyleClass,
};

/// Find the first child template part entity for `parent` tagged with marker `P`.
#[must_use]
pub fn find_template_part<P: Component>(world: &World, parent: Entity) -> Option<Entity> {
    let children = world.get::<Children>(parent)?;
    children
        .iter()
        .find(|child| world.get::<P>(*child).is_some())
}

/// Spawn a new template part under `parent`.
#[must_use]
pub fn spawn_template_part<B: Bundle>(world: &mut World, parent: Entity, bundle: B) -> Entity {
    world.spawn((bundle, ChildOf(parent))).id()
}

/// Ensure a child template part tagged with marker `P` exists.
#[must_use]
pub fn ensure_template_part<P, B>(
    world: &mut World,
    parent: Entity,
    make_bundle: impl FnOnce() -> B,
) -> Entity
where
    P: Component + Default,
    B: Bundle,
{
    if let Some(existing) = find_template_part::<P>(world, parent) {
        return existing;
    }

    spawn_template_part(world, parent, (P::default(), make_bundle()))
}

fn set_label_text(world: &mut World, entity: Entity, text: impl Into<String>) {
    if let Some(mut label) = world.get_mut::<UiLabel>(entity) {
        label.text = text.into();
    }
}

/// Expand built-in logical controls into ECS child template parts.
///
/// This system is intentionally explicit (no macro generation) so applications can
/// copy/adapt the pattern for their own widgets and part markers.
pub fn expand_builtin_control_templates(world: &mut World) {
    let checkboxes = {
        let mut query = world.query::<(Entity, &UiCheckbox)>();
        query
            .iter(world)
            .map(|(entity, checkbox)| (entity, checkbox.label.clone(), checkbox.checked))
            .collect::<Vec<_>>()
    };

    for (entity, label, checked) in checkboxes {
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

        set_label_text(world, indicator, if checked { "☑" } else { "☐" });
        set_label_text(world, label_part, label);
    }

    let sliders = {
        let mut query = world.query::<(Entity, &UiSlider)>();
        query
            .iter(world)
            .map(|(entity, slider)| (entity, slider.value))
            .collect::<Vec<_>>()
    };

    for (entity, value) in sliders {
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

        set_label_text(world, dec, "−");
        set_label_text(world, track, format!("{value:.2}"));
        set_label_text(world, thumb, "●");
        set_label_text(world, inc, "+");
    }

    let switches = {
        let mut query = world.query::<(Entity, &UiSwitch)>();
        query
            .iter(world)
            .map(|(entity, switch)| (entity, switch.on, switch.label.clone()))
            .collect::<Vec<_>>()
    };

    for (entity, on, label) in switches {
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

        let state_text = if on { "On" } else { "Off" };
        let full_text = match label {
            Some(label) if !label.is_empty() => format!("{state_text} · {label}"),
            _ => state_text.to_string(),
        };
        set_label_text(world, track, full_text);
        set_label_text(world, thumb, "●");
    }

    let text_inputs = {
        let mut query = world.query::<(Entity, &UiTextInput)>();
        query
            .iter(world)
            .map(|(entity, input)| (entity, input.placeholder.clone()))
            .collect::<Vec<_>>()
    };

    for (entity, placeholder) in text_inputs {
        let field = ensure_template_part::<PartTextInputField, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.text_input.field".to_string()]),
            )
        });

        set_label_text(world, field, placeholder);
    }

    let dialogs = {
        let mut query = world.query::<(Entity, &UiDialog)>();
        query
            .iter(world)
            .map(|(entity, dialog)| {
                (
                    entity,
                    dialog.title.clone(),
                    dialog.body.clone(),
                    dialog.dismiss_label.clone(),
                )
            })
            .collect::<Vec<_>>()
    };

    for (entity, title, body, dismiss) in dialogs {
        let title_part = ensure_template_part::<PartDialogTitle, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["overlay.dialog.title".to_string()]),
            )
        });
        let body_part = ensure_template_part::<PartDialogBody, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["overlay.dialog.body".to_string()]),
            )
        });
        let dismiss_part = ensure_template_part::<PartDialogDismiss, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["overlay.dialog.dismiss".to_string()]),
            )
        });

        set_label_text(world, title_part, title);
        set_label_text(world, body_part, body);
        set_label_text(world, dismiss_part, dismiss);
    }

    let combo_boxes = {
        let mut query = world.query::<(Entity, &UiComboBox)>();
        query
            .iter(world)
            .map(|(entity, combo)| {
                let display = combo
                    .clamped_selected()
                    .and_then(|index| combo.options.get(index))
                    .map(|opt| opt.label.clone())
                    .unwrap_or_else(|| combo.placeholder.clone());
                let chevron = if combo.is_open { "▴" } else { "▾" };
                (entity, display, chevron.to_string())
            })
            .collect::<Vec<_>>()
    };

    for (entity, display, chevron) in combo_boxes {
        let display_part = ensure_template_part::<PartComboBoxDisplay, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.combo_box.display".to_string()]),
            )
        });
        let chevron_part = ensure_template_part::<PartComboBoxChevron, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.combo_box.chevron".to_string()]),
            )
        });

        set_label_text(world, display_part, display);
        set_label_text(world, chevron_part, chevron);
    }
}
