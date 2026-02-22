use bevy_ecs::{hierarchy::ChildOf, prelude::*};
use bevy_time::Time;

use crate::{
    AnchoredTo, HasTooltip, Hovered, OverlayAnchorRect, OverlayComputedPosition, OverlayConfig,
    OverlayPlacement, OverlayState, UiCheckbox, UiCheckboxChanged, UiOverlayRoot, UiRadioGroup,
    UiRadioGroupChanged, UiSlider, UiSliderChanged, UiSwitch, UiSwitchChanged, UiTabBar,
    UiTabChanged, UiTextInput, UiTextInputChanged, UiToast, UiTooltip, UiTreeNode,
    UiTreeNodeToggled, events::UiEventQueue,
};

/// Internal action enum for non-overlay widget interactions.
///
/// These actions are emitted by built-in widget projectors and consumed by
/// [`handle_widget_actions`] each frame.
#[derive(Debug, Clone, PartialEq)]
pub enum WidgetUiAction {
    /// Select a specific item in a radio group.
    SelectRadioItem { group: Entity, index: usize },
    /// Switch the active tab in a tab bar.
    SelectTab { bar: Entity, index: usize },
    /// Expand or collapse a tree node.
    ToggleTreeNode { node: Entity },
    /// Toggle a checkbox.
    ToggleCheckbox { checkbox: Entity },
    /// Adjust slider value using step increments.
    StepSlider { slider: Entity, delta: f64 },
    /// Toggle a switch.
    ToggleSwitch { switch: Entity },
    /// Update text input contents.
    SetTextInput { input: Entity, value: String },
}

/// Consume [`WidgetUiAction`] entries from [`UiEventQueue`] and apply the
/// corresponding state mutations.
///
/// After mutating each component the system re-emits the appropriate
/// high-level changed event so application code can react to it.
pub fn handle_widget_actions(world: &mut World) {
    let actions = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<WidgetUiAction>();

    for event in actions {
        match event.action {
            WidgetUiAction::SelectRadioItem { group, index } => {
                if world.get_entity(group).is_err() {
                    continue;
                }

                let changed = if let Some(mut radio_group) = world.get_mut::<UiRadioGroup>(group) {
                    radio_group.selected = index;
                    Some(UiRadioGroupChanged {
                        group,
                        selected: index,
                    })
                } else {
                    None
                };

                if let Some(ev) = changed {
                    world.resource::<UiEventQueue>().push_typed(group, ev);
                }
            }

            WidgetUiAction::SelectTab { bar, index } => {
                if world.get_entity(bar).is_err() {
                    continue;
                }

                let changed = if let Some(mut tab_bar) = world.get_mut::<UiTabBar>(bar) {
                    tab_bar.active = index;
                    Some(UiTabChanged { bar, active: index })
                } else {
                    None
                };

                if let Some(ev) = changed {
                    world.resource::<UiEventQueue>().push_typed(bar, ev);
                }
            }

            WidgetUiAction::ToggleTreeNode { node } => {
                if world.get_entity(node).is_err() {
                    continue;
                }

                let toggled = if let Some(tree_node) = world.get::<UiTreeNode>(node) {
                    Some(!tree_node.is_expanded)
                } else {
                    None
                };

                if let Some(is_expanded) = toggled {
                    if let Some(mut tree_node) = world.get_mut::<UiTreeNode>(node) {
                        tree_node.is_expanded = is_expanded;
                    }
                    world
                        .resource::<UiEventQueue>()
                        .push_typed(node, UiTreeNodeToggled { node, is_expanded });
                }
            }

            WidgetUiAction::ToggleCheckbox { checkbox } => {
                if world.get_entity(checkbox).is_err() {
                    continue;
                }

                if let Some(mut checkbox_state) = world.get_mut::<UiCheckbox>(checkbox) {
                    checkbox_state.checked = !checkbox_state.checked;
                    let checked = checkbox_state.checked;
                    drop(checkbox_state);
                    world
                        .resource::<UiEventQueue>()
                        .push_typed(checkbox, UiCheckboxChanged { checkbox, checked });
                }
            }

            WidgetUiAction::StepSlider { slider, delta } => {
                if world.get_entity(slider).is_err() {
                    continue;
                }

                if let Some(mut slider_state) = world.get_mut::<UiSlider>(slider) {
                    let step = slider_state.step.max(f64::EPSILON);
                    let next = (slider_state.value + delta * step)
                        .clamp(slider_state.min, slider_state.max);
                    if (next - slider_state.value).abs() > f64::EPSILON {
                        slider_state.value = next;
                        world.resource::<UiEventQueue>().push_typed(
                            slider,
                            UiSliderChanged {
                                slider,
                                value: next,
                            },
                        );
                    }
                }
            }

            WidgetUiAction::ToggleSwitch { switch } => {
                if world.get_entity(switch).is_err() {
                    continue;
                }

                if let Some(mut switch_state) = world.get_mut::<UiSwitch>(switch) {
                    switch_state.on = !switch_state.on;
                    let on = switch_state.on;
                    drop(switch_state);
                    world
                        .resource::<UiEventQueue>()
                        .push_typed(switch, UiSwitchChanged { switch, on });
                }
            }

            WidgetUiAction::SetTextInput { input, value } => {
                if world.get_entity(input).is_err() {
                    continue;
                }

                if let Some(mut text_input) = world.get_mut::<UiTextInput>(input) {
                    text_input.value = value.clone();
                    world
                        .resource::<UiEventQueue>()
                        .push_typed(input, UiTextInputChanged { input, value });
                }
            }
        }
    }
}

/// Advance toast display timers and despawn any toasts whose duration has elapsed.
///
/// Toasts with `duration_secs == 0.0` are persistent and must be dismissed
/// manually via [`crate::OverlayUiAction::DismissToast`].
pub fn tick_toasts(
    mut commands: Commands,
    mut toasts: Query<(Entity, &mut UiToast)>,
    time: Res<Time>,
) {
    let delta = time.delta_secs();

    for (entity, mut toast) in &mut toasts {
        if toast.duration_secs <= 0.0 {
            continue;
        }

        toast.elapsed_secs += delta;

        if toast.elapsed_secs >= toast.duration_secs {
            commands.entity(entity).despawn();
        }
    }
}

/// Spawn or despawn tooltip overlay entities in response to hover state changes.
///
/// When an entity that carries [`HasTooltip`] gains the [`Hovered`] marker a
/// [`UiTooltip`] overlay is spawned under [`UiOverlayRoot`] anchored to that
/// entity.  When the entity loses the [`Hovered`] marker all tooltip overlays
/// anchored to it are despawned.
pub fn handle_tooltip_hovers(
    mut commands: Commands,
    overlay_root: Query<Entity, With<UiOverlayRoot>>,
    just_hovered: Query<(Entity, &HasTooltip), Added<Hovered>>,
    existing_tooltips: Query<(Entity, &UiTooltip)>,
    mut removed_hover: RemovedComponents<Hovered>,
) {
    // Spawn new tooltips for freshly hovered entities.
    if let Ok(root) = overlay_root.single() {
        for (entity, has_tooltip) in &just_hovered {
            commands.spawn((
                UiTooltip {
                    text: has_tooltip.text.clone(),
                    anchor: entity,
                },
                AnchoredTo(entity),
                OverlayAnchorRect::default(),
                OverlayConfig {
                    placement: OverlayPlacement::Top,
                    anchor: Some(entity),
                    auto_flip: true,
                },
                OverlayState {
                    is_modal: false,
                    anchor: Some(entity),
                },
                OverlayComputedPosition::default(),
                ChildOf(root),
            ));
        }
    }

    // Despawn tooltips whose source entity is no longer hovered.
    let unhovered: Vec<Entity> = removed_hover.read().collect();
    for source in unhovered {
        for (tooltip_entity, tooltip) in &existing_tooltips {
            if tooltip.anchor == source {
                commands.entity(tooltip_entity).despawn();
            }
        }
    }
}
