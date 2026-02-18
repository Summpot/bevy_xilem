use std::collections::HashMap;

use bevy_ecs::{
    bundle::Bundle,
    entity::Entity,
    hierarchy::{ChildOf, Children},
    message::MessageCursor,
    prelude::*,
};
use bevy_input::{
    ButtonInput,
    mouse::{MouseButton, MouseButtonInput},
};
use bevy_math::{Rect, Vec2};
use bevy_window::{PrimaryWindow, Window};
use masonry::core::{Widget, WidgetRef};

use crate::{
    AnchoredTo, AppI18n, AutoDismiss, OverlayAnchorRect, OverlayBounds, OverlayComputedPosition,
    OverlayConfig, OverlayPlacement, StopUiPointerPropagation, UiComboBox, UiComboBoxChanged,
    UiDialog, UiDropdownMenu, UiEventQueue, UiInteractionEvent, UiOverlayRoot, UiPointerEvent,
    UiPointerHitEvent, UiRoot, events::UiEvent, runtime::MasonryRuntime,
    styling::resolve_style_for_classes,
};

const OVERLAY_ANCHOR_GAP: f64 = 4.0;
const DROPDOWN_MAX_VIEWPORT_HEIGHT: f64 = 300.0;
const DIALOG_SURFACE_MIN_WIDTH: f64 = 240.0;
const DIALOG_SURFACE_MAX_WIDTH: f64 = 400.0;

/// Internal overlay actions emitted by built-in floating UI projectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayUiAction {
    DismissDialog,
    ToggleCombo,
    SelectComboItem { index: usize },
    DismissDropdown,
}

/// Per-frame pointer routing decisions used by the input bridge.
#[derive(Resource, Debug, Default)]
pub struct OverlayPointerRoutingState {
    suppressed_presses: Vec<(Entity, MouseButton)>,
    suppressed_releases: Vec<(Entity, MouseButton)>,
}

impl OverlayPointerRoutingState {
    /// Returns true if this exact pressed event should be blocked and consumes the block entry.
    pub(crate) fn take_suppressed_press(&mut self, window: Entity, button: MouseButton) -> bool {
        if let Some(index) = self
            .suppressed_presses
            .iter()
            .position(|(w, b)| *w == window && *b == button)
        {
            self.suppressed_presses.swap_remove(index);
            true
        } else {
            false
        }
    }

    /// Returns true if this exact release event should be blocked and consumes the block entry.
    pub(crate) fn take_suppressed_release(&mut self, window: Entity, button: MouseButton) -> bool {
        if let Some(index) = self
            .suppressed_releases
            .iter()
            .position(|(w, b)| *w == window && *b == button)
        {
            self.suppressed_releases.swap_remove(index);
            true
        } else {
            false
        }
    }
}

/// Message cursor resource used by the world-exclusive click-outside router.
#[derive(Resource, Default)]
pub struct OverlayMouseButtonCursor(pub MessageCursor<MouseButtonInput>);

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

/// Ensure built-in overlays have a default placement and autodismiss policy.
pub fn ensure_overlay_defaults(world: &mut World) {
    let dialogs = {
        let mut query = world.query_filtered::<Entity, With<UiDialog>>();
        query.iter(world).collect::<Vec<_>>()
    };

    for dialog in dialogs {
        if world.get::<OverlayConfig>(dialog).is_none() {
            world.entity_mut(dialog).insert(OverlayConfig {
                placement: OverlayPlacement::Center,
                anchor: None,
                auto_flip: false,
            });
        }
        if world.get::<OverlayComputedPosition>(dialog).is_none() {
            world
                .entity_mut(dialog)
                .insert(OverlayComputedPosition::default());
        }
        if world.get::<AutoDismiss>(dialog).is_none() {
            world.entity_mut(dialog).insert(AutoDismiss);
        }
    }

    let dropdowns = {
        let mut query = world.query::<(Entity, Option<&AnchoredTo>)>();
        query
            .iter(world)
            .filter_map(|(entity, anchored_to)| {
                world
                    .get::<UiDropdownMenu>(entity)
                    .is_some()
                    .then_some((entity, anchored_to.map(|a| a.0)))
            })
            .collect::<Vec<_>>()
    };

    for (dropdown, anchor) in dropdowns {
        if world.get::<OverlayConfig>(dropdown).is_none() {
            world.entity_mut(dropdown).insert(OverlayConfig {
                placement: OverlayPlacement::BottomStart,
                anchor,
                auto_flip: true,
            });
        }

        if world.get::<OverlayComputedPosition>(dropdown).is_none() {
            world
                .entity_mut(dropdown)
                .insert(OverlayComputedPosition::default());
        }

        if world.get::<OverlayAnchorRect>(dropdown).is_none() {
            world
                .entity_mut(dropdown)
                .insert(OverlayAnchorRect::default());
        }

        if world.get::<AutoDismiss>(dropdown).is_none() {
            world.entity_mut(dropdown).insert(AutoDismiss);
        }
    }
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
                if world.get::<UiDialog>(event.entity).is_some() {
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

                let placement = combo.dropdown_placement;
                let auto_flip = combo.auto_flip_placement;

                spawn_in_overlay_root(
                    world,
                    (
                        UiDropdownMenu,
                        AnchoredTo(event.entity),
                        OverlayAnchorRect::default(),
                        OverlayConfig {
                            placement,
                            anchor: Some(event.entity),
                            auto_flip,
                        },
                        OverlayComputedPosition::default(),
                        AutoDismiss,
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

#[derive(Debug, Clone, Copy)]
struct EntityHitBox {
    entity: Entity,
    rect: OverlayAnchorRect,
}

fn parse_entity_from_ecs_button(widget: WidgetRef<'_, dyn Widget>) -> Option<Entity> {
    if widget.short_type_name() != "EcsButtonWidget" {
        return None;
    }

    let debug = widget.get_debug_text()?;
    let bits = debug.strip_prefix("entity=")?.parse::<u64>().ok()?;
    Entity::try_from_bits(bits)
}

fn collect_entity_hit_boxes(widget: WidgetRef<'_, dyn Widget>, out: &mut Vec<EntityHitBox>) {
    for child in widget.children() {
        collect_entity_hit_boxes(child, out);
    }

    let Some(entity) = parse_entity_from_ecs_button(widget) else {
        return;
    };

    let ctx = widget.ctx();
    let origin = ctx.window_origin();
    let size = ctx.border_box_size();
    out.push(EntityHitBox {
        entity,
        rect: OverlayAnchorRect {
            left: origin.x,
            top: origin.y,
            width: size.width,
            height: size.height,
        },
    });
}

fn is_point_in_rect(rect: OverlayAnchorRect, x: f64, y: f64) -> bool {
    x >= rect.left && x <= rect.left + rect.width && y >= rect.top && y <= rect.top + rect.height
}

fn translate_text(world: &World, key: Option<&str>, fallback: &str) -> String {
    match key {
        Some(key) => world.get_resource::<AppI18n>().map_or_else(
            || {
                if fallback.is_empty() {
                    key.to_string()
                } else {
                    fallback.to_string()
                }
            },
            |i18n| i18n.translate(key),
        ),
        None => fallback.to_string(),
    }
}

fn estimate_text_width_px(text: &str, font_size: f32) -> f64 {
    let units = text
        .chars()
        .map(|ch| {
            if ch.is_ascii_whitespace() {
                0.34
            } else if ch.is_ascii() {
                0.56
            } else {
                1.0
            }
        })
        .sum::<f64>();

    (units * font_size as f64).max(font_size as f64 * 2.0)
}

fn estimate_wrapped_lines(text: &str, font_size: f32, max_line_width: f64) -> usize {
    let max_line_width = max_line_width.max(font_size as f64 * 2.0);
    let mut total = 0_usize;

    for raw_line in text.lines() {
        let logical_line = if raw_line.is_empty() { " " } else { raw_line };
        let width = estimate_text_width_px(logical_line, font_size);
        let wrapped = (width / max_line_width).ceil() as usize;
        total += wrapped.max(1);
    }

    total.max(1)
}

fn estimate_dialog_surface_width_px(
    title: &str,
    body: &str,
    dismiss_label: &str,
    title_size: f32,
    body_size: f32,
    dismiss_size: f32,
    horizontal_padding: f64,
) -> f64 {
    let mut widest = estimate_text_width_px(title, title_size)
        .max(estimate_text_width_px(dismiss_label, dismiss_size));

    for line in body.lines() {
        widest = widest.max(estimate_text_width_px(line, body_size));
    }

    (widest + horizontal_padding * 2.0 + 40.0)
        .clamp(DIALOG_SURFACE_MIN_WIDTH, DIALOG_SURFACE_MAX_WIDTH)
}

fn estimate_dialog_surface_height_px(
    title: &str,
    body: &str,
    dialog_surface_width: f64,
    title_size: f32,
    body_size: f32,
    dismiss_size: f32,
    dismiss_padding: f64,
    gap: f64,
    horizontal_padding: f64,
    vertical_padding: f64,
) -> f64 {
    let title_line_height = (title_size as f64 * 1.35).max(18.0);
    let body_line_height = (body_size as f64 * 1.45).max(18.0);
    let dismiss_height = (dismiss_size as f64 * 1.25 + dismiss_padding * 2.0).max(30.0);

    let text_max_width = (dialog_surface_width - horizontal_padding * 2.0 - 8.0).max(120.0);
    let title_lines = estimate_wrapped_lines(title, title_size, text_max_width);
    let body_lines = estimate_wrapped_lines(body, body_size, text_max_width);

    (vertical_padding * 2.0
        + title_lines as f64 * title_line_height
        + body_lines as f64 * body_line_height
        + dismiss_height
        + gap * 2.0)
        .max(120.0)
}

fn estimate_dropdown_surface_width_px<'a>(
    anchor_width: f64,
    labels: impl IntoIterator<Item = &'a str>,
    font_size: f32,
    horizontal_padding: f64,
) -> f64 {
    let widest_label = labels
        .into_iter()
        .map(|label| estimate_text_width_px(label, font_size))
        .fold(0.0, f64::max);

    (widest_label + horizontal_padding + 24.0).max(anchor_width.max(1.0))
}

fn estimate_dropdown_viewport_height_px(
    item_count: usize,
    item_font_size: f32,
    item_padding: f64,
    item_gap: f64,
) -> f64 {
    let per_item = (item_font_size as f64 + item_padding * 2.0 + 8.0).max(28.0);
    let gap_total = item_gap * item_count.saturating_sub(1) as f64;
    let content_height = per_item * item_count as f64 + gap_total;
    content_height.clamp(per_item, DROPDOWN_MAX_VIEWPORT_HEIGHT)
}

fn overlay_size_for_entity(
    world: &World,
    entity: Entity,
    anchor_rects: &HashMap<Entity, OverlayAnchorRect>,
) -> (f64, f64) {
    if let Some(dialog) = world.get::<UiDialog>(entity) {
        let mut dialog_style = resolve_style_for_classes(world, ["overlay.dialog.surface"]);
        let mut title_style = resolve_style_for_classes(world, ["overlay.dialog.title"]);
        let mut body_style = resolve_style_for_classes(world, ["overlay.dialog.body"]);
        let mut dismiss_style = resolve_style_for_classes(world, ["overlay.dialog.dismiss"]);

        if dialog_style.layout.padding <= 0.0 {
            dialog_style.layout.padding = 18.0;
        }
        if dialog_style.layout.gap <= 0.0 {
            dialog_style.layout.gap = 10.0;
        }
        if dismiss_style.layout.padding <= 0.0 {
            dismiss_style.layout.padding = 8.0;
        }

        if title_style.text.size <= 0.0 {
            title_style.text.size = 24.0;
        }
        if body_style.text.size <= 0.0 {
            body_style.text.size = 16.0;
        }
        if dismiss_style.text.size <= 0.0 {
            dismiss_style.text.size = 15.0;
        }

        let title = translate_text(world, dialog.title_key.as_deref(), &dialog.title);
        let body = translate_text(world, dialog.body_key.as_deref(), &dialog.body);
        let dismiss_label =
            translate_text(world, dialog.dismiss_key.as_deref(), &dialog.dismiss_label);

        let width = estimate_dialog_surface_width_px(
            &title,
            &body,
            &dismiss_label,
            title_style.text.size,
            body_style.text.size,
            dismiss_style.text.size,
            dialog_style.layout.padding.max(12.0),
        );

        let height = estimate_dialog_surface_height_px(
            &title,
            &body,
            width,
            title_style.text.size,
            body_style.text.size,
            dismiss_style.text.size,
            dismiss_style.layout.padding.max(8.0),
            dialog_style.layout.gap.max(10.0),
            dialog_style.layout.padding.max(12.0),
            dialog_style.layout.padding.max(12.0),
        );

        return (width, height);
    }

    if world.get::<UiDropdownMenu>(entity).is_some() {
        let Some(anchor) = world.get::<AnchoredTo>(entity).map(|a| a.0) else {
            return (220.0, 120.0);
        };

        let Some(combo_box) = world.get::<UiComboBox>(anchor) else {
            return (220.0, 120.0);
        };

        let item_style = resolve_style_for_classes(world, ["overlay.dropdown.item"]);
        let menu_style = resolve_style_for_classes(world, ["overlay.dropdown.menu"]);

        let translated_options = combo_box
            .options
            .iter()
            .map(|option| translate_text(world, option.label_key.as_deref(), &option.label))
            .collect::<Vec<_>>();

        let anchor_width = anchor_rects
            .get(&anchor)
            .map(|rect| rect.width)
            .unwrap_or(160.0);

        let width = estimate_dropdown_surface_width_px(
            anchor_width,
            translated_options.iter().map(String::as_str),
            item_style.text.size.max(16.0),
            item_style.layout.padding * 2.0 + menu_style.layout.padding * 2.0,
        );

        let item_gap = menu_style.layout.gap.max(6.0);
        let height = estimate_dropdown_viewport_height_px(
            translated_options.len(),
            item_style.text.size.max(16.0),
            item_style.layout.padding.max(8.0),
            item_gap,
        );

        return (width, height);
    }

    (240.0, 120.0)
}

fn overlay_origin_for_placement(
    placement: OverlayPlacement,
    anchor_rect: OverlayAnchorRect,
    overlay_width: f64,
    overlay_height: f64,
    gap: f64,
) -> (f64, f64) {
    let start_x = anchor_rect.left;
    let centered_x = anchor_rect.left + (anchor_rect.width - overlay_width) * 0.5;
    let end_x = anchor_rect.left + anchor_rect.width - overlay_width;

    let top_y = anchor_rect.top - overlay_height - gap;
    let centered_y = anchor_rect.top + (anchor_rect.height - overlay_height) * 0.5;
    let bottom_y = anchor_rect.top + anchor_rect.height + gap;

    match placement {
        OverlayPlacement::Center => (centered_x, centered_y),
        OverlayPlacement::Top => (centered_x, top_y),
        OverlayPlacement::Bottom => (centered_x, bottom_y),
        OverlayPlacement::Left => (anchor_rect.left - overlay_width - gap, centered_y),
        OverlayPlacement::Right => (anchor_rect.left + anchor_rect.width + gap, centered_y),
        OverlayPlacement::TopStart => (start_x, top_y),
        OverlayPlacement::TopEnd => (end_x, top_y),
        OverlayPlacement::BottomStart => (start_x, bottom_y),
        OverlayPlacement::BottomEnd => (end_x, bottom_y),
        OverlayPlacement::LeftStart => (anchor_rect.left - overlay_width - gap, anchor_rect.top),
        OverlayPlacement::RightStart => {
            (anchor_rect.left + anchor_rect.width + gap, anchor_rect.top)
        }
    }
}

fn flip_placement(placement: OverlayPlacement) -> Option<OverlayPlacement> {
    Some(match placement {
        OverlayPlacement::Top => OverlayPlacement::Bottom,
        OverlayPlacement::Bottom => OverlayPlacement::Top,
        OverlayPlacement::TopStart => OverlayPlacement::BottomStart,
        OverlayPlacement::TopEnd => OverlayPlacement::BottomEnd,
        OverlayPlacement::BottomStart => OverlayPlacement::TopStart,
        OverlayPlacement::BottomEnd => OverlayPlacement::TopEnd,
        OverlayPlacement::Left => OverlayPlacement::Right,
        OverlayPlacement::Right => OverlayPlacement::Left,
        OverlayPlacement::LeftStart => OverlayPlacement::RightStart,
        OverlayPlacement::RightStart => OverlayPlacement::LeftStart,
        OverlayPlacement::Center => return None,
    })
}

fn visible_area(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    viewport_width: f64,
    viewport_height: f64,
) -> f64 {
    let left = x.max(0.0);
    let top = y.max(0.0);
    let right = (x + width).min(viewport_width);
    let bottom = (y + height).min(viewport_height);

    let visible_width = (right - left).max(0.0);
    let visible_height = (bottom - top).max(0.0);
    visible_width * visible_height
}

fn overflows_bottom(y: f64, height: f64, viewport_height: f64) -> bool {
    y + height > viewport_height
}

fn clamp_overlay_origin(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    viewport_width: f64,
    viewport_height: f64,
) -> (f64, f64) {
    let max_x = (viewport_width - width).max(0.0);
    let max_y = (viewport_height - height).max(0.0);
    (x.clamp(0.0, max_x), y.clamp(0.0, max_y))
}

/// Universal placement + collision-detection system for overlay entities.
///
/// Runs after layout/input updates and computes final window-space coordinates that
/// projectors apply to overlay surfaces.
pub fn sync_overlay_positions(world: &mut World) {
    let overlays = {
        let mut query = world.query::<(Entity, &OverlayConfig)>();
        query
            .iter(world)
            .map(|(entity, config)| (entity, *config))
            .collect::<Vec<_>>()
    };

    tracing::debug!(
        "Running sync_overlay_positions for {} overlays",
        overlays.iter().count()
    );

    if overlays.is_empty() {
        return;
    }

    let (viewport_width, viewport_height) = {
        let mut window_query = world.query_filtered::<&Window, With<PrimaryWindow>>();
        let Some(window) = window_query.iter(world).next() else {
            return;
        };

        let window_width = window.width() as f64;
        let window_height = window.height() as f64;
        tracing::debug!("Dynamic Window Size: {}x{}", window_width, window_height);
        (window_width, window_height)
    };

    let hit_boxes = {
        let runtime = world.non_send_resource::<MasonryRuntime>();
        let root = runtime.render_root.get_layer_root(0);
        let mut boxes = Vec::new();
        collect_entity_hit_boxes(root, &mut boxes);
        boxes
    };

    let mut anchor_rects = HashMap::new();
    for hit in hit_boxes {
        anchor_rects.insert(hit.entity, hit.rect);
    }

    let mut stale_dropdowns = Vec::new();

    for (entity, config) in overlays {
        if world.get_entity(entity).is_err() {
            continue;
        }

        let (width, height) = overlay_size_for_entity(world, entity, &anchor_rects);

        let (anchor_rect, anchor_gap) = if let Some(anchor) = config.anchor {
            let Some(anchor_rect) = anchor_rects.get(&anchor).copied() else {
                tracing::warn!(
                    "Anchor entity {:?} geometry resolution failed (missing GlobalTransform/Node/hit-box)",
                    anchor
                );
                if world.get::<UiDropdownMenu>(entity).is_some() {
                    stale_dropdowns.push(entity);
                }
                continue;
            };
            tracing::debug!(
                "Anchor entity {:?} global bounds: {:?}",
                anchor,
                anchor_rect
            );
            (anchor_rect, OVERLAY_ANCHOR_GAP)
        } else {
            (
                OverlayAnchorRect {
                    left: 0.0,
                    top: 0.0,
                    width: viewport_width,
                    height: viewport_height,
                },
                0.0,
            )
        };

        let mut chosen_placement = config.placement;
        let mut did_flip = false;
        let (mut x, mut y) =
            overlay_origin_for_placement(config.placement, anchor_rect, width, height, anchor_gap);

        if config.auto_flip
            && overflows_bottom(y, height, viewport_height)
            && let Some(flipped) = flip_placement(config.placement)
        {
            let (fx, fy) =
                overlay_origin_for_placement(flipped, anchor_rect, width, height, anchor_gap);

            let preferred_visible =
                visible_area(x, y, width, height, viewport_width, viewport_height);
            let flipped_visible =
                visible_area(fx, fy, width, height, viewport_width, viewport_height);

            if flipped_visible > preferred_visible {
                x = fx;
                y = fy;
                chosen_placement = flipped;
                did_flip = true;
            }
        }

        let (x, y) = clamp_overlay_origin(
            x,
            y,
            width,
            height,
            viewport_width.max(1.0),
            viewport_height.max(1.0),
        );

        let final_rect = OverlayAnchorRect {
            left: x,
            top: y,
            width,
            height,
        };
        tracing::debug!(
            "Calculated overlay rect: {:?}, Auto-flip triggered: {}",
            final_rect,
            did_flip
        );

        if let Some(mut computed) = world.get_mut::<OverlayComputedPosition>(entity) {
            *computed = OverlayComputedPosition {
                x,
                y,
                width,
                height,
                placement: chosen_placement,
            };
        } else {
            world.entity_mut(entity).insert(OverlayComputedPosition {
                x,
                y,
                width,
                height,
                placement: chosen_placement,
            });
        }

        let bounds_rect = Rect::from_corners(
            Vec2::new(x as f32, y as f32),
            Vec2::new((x + width) as f32, (y + height) as f32),
        );

        if let Some(mut bounds) = world.get_mut::<OverlayBounds>(entity) {
            bounds.rect = bounds_rect;
        } else {
            world
                .entity_mut(entity)
                .insert(OverlayBounds { rect: bounds_rect });
        }

        if let Some(anchor) = config.anchor
            && let Some(anchor_rect) = anchor_rects.get(&anchor).copied()
        {
            if let Some(mut cached_anchor) = world.get_mut::<OverlayAnchorRect>(entity) {
                *cached_anchor = anchor_rect;
            } else {
                world.entity_mut(entity).insert(anchor_rect);
            }
        }
    }

    for stale in stale_dropdowns {
        if world.get_entity(stale).is_ok() {
            close_dropdown(world, stale);
        }
    }
}

/// Backward-compatible alias kept for existing callsites.
pub fn sync_dropdown_positions(world: &mut World) {
    sync_overlay_positions(world);
}

/// Native Bevy click-outside dismissal using cursor + [`OverlayBounds`] intersection tests.
pub fn native_dismiss_overlays_on_click(world: &mut World) {
    let left_just_pressed = {
        let Some(mouse_input) = world.get_resource::<ButtonInput<MouseButton>>() else {
            return;
        };
        mouse_input.just_pressed(MouseButton::Left)
    };

    if !left_just_pressed {
        return;
    }

    let cursor_pos = {
        let mut window_query = world.query_filtered::<&Window, With<PrimaryWindow>>();
        let Some(window) = window_query.iter(world).next() else {
            return;
        };
        let Some(cursor_pos) = window.cursor_position() else {
            return;
        };

        tracing::debug!("Native click detected at: {:?}", cursor_pos);
        cursor_pos
    };

    let overlays = {
        let mut query = world.query::<(
            Entity,
            &OverlayConfig,
            Option<&AutoDismiss>,
            Option<&OverlayBounds>,
            Option<&OverlayAnchorRect>,
        )>();

        query
            .iter(world)
            .map(|(entity, config, autodismiss, bounds, anchor_rect)| {
                (
                    entity,
                    *config,
                    autodismiss.is_some(),
                    bounds.map(|bounds| bounds.rect),
                    anchor_rect.copied(),
                )
            })
            .collect::<Vec<_>>()
    };

    if overlays.is_empty() {
        return;
    }

    let cursor_x = cursor_pos.x as f64;
    let cursor_y = cursor_pos.y as f64;
    let mut overlays_to_close = Vec::new();

    for (entity, config, auto_dismiss, bounds, anchor_rect) in overlays {
        if !auto_dismiss || world.get_entity(entity).is_err() {
            continue;
        }

        let clicked_inside_overlay = bounds.is_some_and(|rect| rect.contains(cursor_pos));
        let clicked_inside_anchor = config.anchor.is_some()
            && anchor_rect.is_some_and(|rect| is_point_in_rect(rect, cursor_x, cursor_y));

        if !clicked_inside_overlay && !clicked_inside_anchor {
            overlays_to_close.push(entity);
        }
    }

    for entity in overlays_to_close {
        if world.get_entity(entity).is_err() {
            continue;
        }

        tracing::debug!("Click outside detected! Despawning overlay {:?}", entity);
        if world.get::<UiDropdownMenu>(entity).is_some() {
            close_dropdown(world, entity);
        } else {
            despawn_entity_tree(world, entity);
        }
    }
}

/// Backward-compatible alias kept for existing callsites.
pub fn dismiss_overlays_on_click(world: &mut World) {
    native_dismiss_overlays_on_click(world);
}

/// Bubble pointer hits up the ECS parent hierarchy, emitting [`UiPointerEvent`] entries.
pub fn bubble_ui_pointer_events(world: &mut World) {
    let hits = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiPointerHitEvent>();

    if hits.is_empty() {
        return;
    }

    for hit in hits {
        if world.get_entity(hit.action.target).is_err() {
            continue;
        }

        let mut current = Some(hit.action.target);

        while let Some(current_entity) = current {
            let consumed = world
                .get::<StopUiPointerPropagation>(current_entity)
                .is_some();

            world.resource::<UiEventQueue>().push(UiEvent::typed(
                current_entity,
                UiPointerEvent {
                    target: hit.action.target,
                    current_target: current_entity,
                    position: hit.action.position,
                    button: hit.action.button,
                    phase: hit.action.phase,
                    consumed,
                },
            ));

            if consumed {
                break;
            }

            current = world
                .get::<ChildOf>(current_entity)
                .map(|child_of| child_of.parent());
        }
    }
}

/// Keep pseudo-state interaction queue alive when raw pointer events are consumed.
///
/// If we suppress a pointer click before it reaches Masonry, we still clear stale pressed
/// marker transitions to avoid sticky visual states.
pub fn clear_stale_pressed_interactions(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiInteractionEvent>();

    for event in events {
        world
            .resource::<UiEventQueue>()
            .push_typed(event.entity, event.action);
    }
}
