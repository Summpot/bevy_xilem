use std::collections::HashMap;

use bevy_ecs::{
    bundle::Bundle,
    entity::Entity,
    hierarchy::{ChildOf, Children},
    prelude::*,
};
use masonry::core::{Widget, WidgetRef};

use crate::{
    AnchoredTo, OverlayAnchorRect, UiComboBox, UiComboBoxChanged, UiDialog, UiDropdownMenu,
    UiEventQueue, UiOverlayRoot, UiRoot, runtime::MasonryRuntime, widgets::EcsButtonWidget,
};

/// Internal overlay actions emitted by built-in floating UI projectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayUiAction {
    DismissDialog,
    ToggleCombo,
    SelectComboItem { index: usize },
    DismissDropdown,
}

fn first_overlay_root(world: &mut World) -> Option<Entity> {
    let mut query = world.query_filtered::<Entity, With<UiOverlayRoot>>();
    query.iter(world).next()
}

/// Ensure an overlay root exists and return its entity id.
pub fn ensure_overlay_root_entity(world: &mut World) -> Entity {
    if let Some(existing) = first_overlay_root(world) {
        return existing;
    }

    world.spawn((UiRoot, UiOverlayRoot)).id()
}

/// Spawn an entity bundle under the global overlay root.
///
/// This is the recommended entrypoint for app-level modal/dropdown/tooltips.
pub fn spawn_in_overlay_root<B: Bundle>(world: &mut World, bundle: B) -> Entity {
    let overlay_root = ensure_overlay_root_entity(world);
    world.spawn((bundle, ChildOf(overlay_root))).id()
}

fn collect_dropdowns_for_combo(world: &mut World, combo: Entity) -> Vec<Entity> {
    let mut query = world.query::<(Entity, &AnchoredTo, &UiDropdownMenu)>();
    query
        .iter(world)
        .filter_map(|(entity, anchored_to, _)| (anchored_to.0 == combo).then_some(entity))
        .collect()
}

fn despawn_entity_tree(world: &mut World, entity: Entity) {
    let children = world
        .get::<Children>(entity)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();

    for child in children {
        if world.get_entity(child).is_ok() {
            despawn_entity_tree(world, child);
        }
    }

    let _ = world.despawn(entity);
}

fn close_dropdown(world: &mut World, dropdown_entity: Entity) {
    let anchor = world
        .get::<AnchoredTo>(dropdown_entity)
        .map(|anchored| anchored.0);

    despawn_entity_tree(world, dropdown_entity);

    if let Some(anchor) = anchor
        && let Some(mut combo_box) = world.get_mut::<UiComboBox>(anchor)
    {
        combo_box.is_open = false;
    }
}

/// Ensure a global [`UiOverlayRoot`] exists whenever there is at least one regular [`UiRoot`].
pub fn ensure_overlay_root(world: &mut World) {
    if first_overlay_root(world).is_some() {
        return;
    }

    let has_regular_root = {
        let mut query = world.query_filtered::<Entity, (With<UiRoot>, Without<UiOverlayRoot>)>();
        query.iter(world).next().is_some()
    };

    if !has_regular_root {
        return;
    }

    world.spawn((UiRoot, UiOverlayRoot));
}

/// Move built-in overlay entities under [`UiOverlayRoot`], creating one if needed.
///
/// This keeps modal/dropdown ownership internal to the library and avoids app-level
/// overlay root plumbing for common cases.
pub fn reparent_overlay_entities(world: &mut World) {
    let overlay_entities = {
        let mut query = world.query_filtered::<Entity, (
            Or<(With<UiDialog>, With<UiDropdownMenu>)>,
            Without<UiOverlayRoot>,
        )>();
        query.iter(world).collect::<Vec<_>>()
    };

    if overlay_entities.is_empty() {
        return;
    }

    let overlay_root = ensure_overlay_root_entity(world);

    for entity in overlay_entities {
        let already_parented = world
            .get::<ChildOf>(entity)
            .is_some_and(|child_of| child_of.parent() == overlay_root);
        if already_parented {
            continue;
        }

        if world.get_entity(entity).is_ok() {
            world.entity_mut(entity).insert(ChildOf(overlay_root));
        }
    }
}

/// Consume built-in overlay actions and mutate ECS overlay state.
pub fn handle_overlay_actions(world: &mut World) {
    let actions = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<OverlayUiAction>();

    for event in actions {
        if world.get_entity(event.entity).is_err() {
            continue;
        }

        match event.action {
            OverlayUiAction::DismissDialog => {
                if world.get::<crate::UiDialog>(event.entity).is_some() {
                    despawn_entity_tree(world, event.entity);
                }
            }
            OverlayUiAction::ToggleCombo => {
                let Some(combo) = world.get::<UiComboBox>(event.entity).cloned() else {
                    continue;
                };

                let existing_dropdowns = collect_dropdowns_for_combo(world, event.entity);
                for dropdown in existing_dropdowns {
                    if world.get_entity(dropdown).is_ok() {
                        close_dropdown(world, dropdown);
                    }
                }

                if combo.is_open {
                    if let Some(mut combo_box) = world.get_mut::<UiComboBox>(event.entity) {
                        combo_box.is_open = false;
                    }
                    continue;
                }

                spawn_in_overlay_root(
                    world,
                    (
                        UiDropdownMenu,
                        AnchoredTo(event.entity),
                        OverlayAnchorRect::default(),
                    ),
                );

                if let Some(mut combo_box) = world.get_mut::<UiComboBox>(event.entity) {
                    combo_box.is_open = true;
                }
            }
            OverlayUiAction::SelectComboItem { index } => {
                let Some(anchor) = world
                    .get::<AnchoredTo>(event.entity)
                    .map(|anchored| anchored.0)
                else {
                    continue;
                };

                let mut changed_event = None;
                if let Some(mut combo_box) = world.get_mut::<UiComboBox>(anchor)
                    && !combo_box.options.is_empty()
                {
                    let selected = index.min(combo_box.options.len() - 1);
                    combo_box.selected = selected;
                    changed_event = Some(UiComboBoxChanged {
                        combo: anchor,
                        selected,
                        value: combo_box.options[selected].value.clone(),
                    });
                }

                if let Some(changed_event) = changed_event {
                    world
                        .resource::<UiEventQueue>()
                        .push_typed(anchor, changed_event);
                }

                if world.get_entity(event.entity).is_ok() {
                    close_dropdown(world, event.entity);
                }
            }
            OverlayUiAction::DismissDropdown => {
                if world.get_entity(event.entity).is_ok()
                    && world.get::<UiDropdownMenu>(event.entity).is_some()
                {
                    close_dropdown(world, event.entity);
                }
            }
        }
    }
}

fn anchor_rect_for_entity(
    widget: WidgetRef<'_, dyn Widget>,
    entity: Entity,
) -> Option<OverlayAnchorRect> {
    if let Some(button) = widget.downcast::<EcsButtonWidget<OverlayUiAction>>()
        && button.inner().entity() == entity
    {
        let ctx = button.ctx();
        let origin = ctx.window_origin();
        let size = ctx.border_box_size();
        return Some(OverlayAnchorRect {
            left: origin.x,
            top: origin.y,
            width: size.width,
            height: size.height,
        });
    }

    for child in widget.children() {
        if let Some(found) = anchor_rect_for_entity(child, entity) {
            return Some(found);
        }
    }

    None
}

/// Update anchored dropdown menu positions from Masonry's latest widget layout.
///
/// This uses window-space bounds of combo anchor widgets and updates each
/// [`OverlayAnchorRect`] so dropdown menus can follow anchors across resize/scroll/layout.
pub fn sync_dropdown_positions(world: &mut World) {
    let dropdowns = {
        let mut query = world.query::<(Entity, &UiDropdownMenu, &AnchoredTo)>();
        query
            .iter(world)
            .map(|(entity, _, anchored_to)| (entity, anchored_to.0))
            .collect::<Vec<_>>()
    };

    if dropdowns.is_empty() {
        return;
    }

    let anchor_ids = dropdowns
        .iter()
        .map(|(_, anchor)| *anchor)
        .collect::<Vec<_>>();

    let anchor_rects = {
        let runtime = world.non_send_resource::<MasonryRuntime>();
        let root = runtime.render_root.get_layer_root(0);
        let mut rects = HashMap::with_capacity(anchor_ids.len());
        for anchor in anchor_ids {
            rects
                .entry(anchor)
                .or_insert_with(|| anchor_rect_for_entity(root, anchor));
        }
        rects
    };

    let mut stale_dropdowns = Vec::new();

    for (dropdown_entity, anchor_entity) in dropdowns {
        let Some(rect) = anchor_rects.get(&anchor_entity).copied().flatten() else {
            stale_dropdowns.push(dropdown_entity);
            continue;
        };

        if let Some(mut anchor_rect) = world.get_mut::<OverlayAnchorRect>(dropdown_entity) {
            if *anchor_rect != rect {
                *anchor_rect = rect;
            }
        } else if world.get_entity(dropdown_entity).is_ok() {
            world.entity_mut(dropdown_entity).insert(rect);
        }
    }

    for stale_dropdown in stale_dropdowns {
        if world.get_entity(stale_dropdown).is_ok() {
            close_dropdown(world, stale_dropdown);
        }
    }
}
