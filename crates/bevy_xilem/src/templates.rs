use bevy_ecs::{
    entity::Entity,
    hierarchy::{ChildOf, Children},
    prelude::*,
};

use crate::{
    controls::expand_all_ui_control_templates,
    ecs::{UiCheckbox, UiComboBox, UiDialog, UiSlider, UiSwitch, UiTextInput},
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

/// Compatibility helper: expand built-in logical controls into ECS child template parts.
///
/// New code should prefer trait-driven registration (`register_ui_control::<T>()`),
/// which installs `Added<T>` expansion systems automatically.
pub fn expand_builtin_control_templates(world: &mut World) {
    expand_all_ui_control_templates::<UiCheckbox>(world);
    expand_all_ui_control_templates::<UiSlider>(world);
    expand_all_ui_control_templates::<UiSwitch>(world);
    expand_all_ui_control_templates::<UiTextInput>(world);
    expand_all_ui_control_templates::<UiDialog>(world);
    expand_all_ui_control_templates::<UiComboBox>(world);
}
