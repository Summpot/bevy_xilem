use std::{fmt, marker::PhantomData, sync::Arc};

use bevy_ecs::prelude::*;
use masonry::layout::{Dim, Length, UnitPoint};
use tracing::trace;
use xilem::{palette::css::BLACK, style::BoxShadow, style::Style as _};
use xilem_masonry::{
    AnyWidgetView,
    view::{
        CrossAxisAlignment, FlexExt as _, ZStackExt as _, flex_col, flex_row, label, portal,
        transformed, zstack,
    },
};

use crate::{
    ecs::{
        AnchoredTo, LocalizeText, OverlayAnchorRect, OverlayComputedPosition, UiButton, UiComboBox,
        UiDialog, UiDropdownMenu, UiFlexColumn, UiFlexRow, UiLabel, UiOverlayRoot,
    },
    i18n::{AppI18n, resolve_localized_text},
    overlay::OverlayUiAction,
    styling::{
        apply_direct_widget_style, apply_label_style, apply_widget_style, resolve_style,
        resolve_style_for_classes,
    },
    views::{ecs_button, ecs_button_with_child},
};

#[cfg(test)]
use crate::UiDropdownPlacement;

/// Xilem state used by synthesized UI views.
pub type UiXilemState = ();
/// Xilem action type used by synthesized UI views.
pub type UiXilemAction = ();

/// Type-erased Xilem Masonry view used as projection output.
pub type UiAnyView = AnyWidgetView<UiXilemState, UiXilemAction>;
/// Shared synthesized view handle.
pub type UiView = Arc<UiAnyView>;

/// Built-in button action emitted by [`UiButton`] projector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinUiAction {
    Clicked,
}

/// Projection context passed to projector implementations.
pub struct ProjectionCtx<'a> {
    pub world: &'a World,
    pub entity: Entity,
    pub node_id: u64,
    pub children: Vec<UiView>,
}

impl fmt::Debug for ProjectionCtx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjectionCtx")
            .field("entity", &self.entity)
            .field("node_id", &self.node_id)
            .field("children_len", &self.children.len())
            .finish()
    }
}

/// Maps ECS entity data into a concrete Xilem Masonry view.
pub trait UiProjector: Send + Sync + 'static {
    fn project(&self, ctx: ProjectionCtx<'_>) -> Option<UiView>;
}

struct ComponentProjector<C: Component> {
    projector: fn(&C, ProjectionCtx<'_>) -> UiView,
    _marker: PhantomData<C>,
}

impl<C: Component> UiProjector for ComponentProjector<C> {
    fn project(&self, ctx: ProjectionCtx<'_>) -> Option<UiView> {
        let component = ctx.world.get::<C>(ctx.entity)?;
        Some((self.projector)(component, ctx))
    }
}

/// Registry of projector implementations.
#[derive(Resource, Default)]
pub struct UiProjectorRegistry {
    projectors: Vec<Box<dyn UiProjector>>,
}

impl UiProjectorRegistry {
    /// Register a raw projector implementation.
    pub fn register_projector<P: UiProjector>(&mut self, projector: P) -> &mut Self {
        self.projectors.push(Box::new(projector));
        self
    }

    /// Register a projector bound to a specific ECS component type.
    pub fn register_component<C: Component>(
        &mut self,
        projector: fn(&C, ProjectionCtx<'_>) -> UiView,
    ) -> &mut Self {
        self.register_projector(ComponentProjector::<C> {
            projector,
            _marker: PhantomData,
        })
    }

    pub(crate) fn project_node(
        &self,
        world: &World,
        entity: Entity,
        node_id: u64,
        children: Vec<UiView>,
    ) -> Option<UiView> {
        // Last registered projector wins.
        for projector in self.projectors.iter().rev() {
            let ctx = ProjectionCtx {
                world,
                entity,
                node_id,
                children: children.clone(),
            };
            if let Some(view) = projector.project(ctx) {
                return Some(view);
            }
        }

        None
    }
}

fn project_flex_column(_: &UiFlexColumn, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_col(children).gap(Length::px(style.layout.gap)),
        &style,
    ))
}

fn project_flex_row(_: &UiFlexRow, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_row(children).gap(Length::px(style.layout.gap)),
        &style,
    ))
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

const DIALOG_SURFACE_MIN_WIDTH: f64 = 240.0;
const DIALOG_SURFACE_MAX_WIDTH: f64 = 400.0;
const DROPDOWN_MAX_VIEWPORT_HEIGHT: f64 = 300.0;
#[cfg(test)]
const OVERLAY_ANCHOR_GAP: f64 = 4.0;

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

#[cfg(test)]
fn dropdown_origin_for_placement(
    anchor_rect: OverlayAnchorRect,
    dropdown_width: f64,
    dropdown_height: f64,
    placement: UiDropdownPlacement,
) -> (f64, f64) {
    let start_x = anchor_rect.left;
    let centered_x = anchor_rect.left + (anchor_rect.width - dropdown_width) * 0.5;
    let end_x = anchor_rect.left + anchor_rect.width - dropdown_width;
    let centered_y = anchor_rect.top + (anchor_rect.height - dropdown_height) * 0.5;
    let bottom_y = anchor_rect.top + anchor_rect.height + OVERLAY_ANCHOR_GAP;
    let top_y = anchor_rect.top - dropdown_height - OVERLAY_ANCHOR_GAP;

    match placement {
        UiDropdownPlacement::Center => (centered_x, centered_y),
        UiDropdownPlacement::Left => (
            anchor_rect.left - dropdown_width - OVERLAY_ANCHOR_GAP,
            centered_y,
        ),
        UiDropdownPlacement::Right => (
            anchor_rect.left + anchor_rect.width + OVERLAY_ANCHOR_GAP,
            centered_y,
        ),
        UiDropdownPlacement::BottomStart => (start_x, bottom_y),
        UiDropdownPlacement::Bottom => (centered_x, bottom_y),
        UiDropdownPlacement::BottomEnd => (end_x, bottom_y),
        UiDropdownPlacement::TopStart => (start_x, top_y),
        UiDropdownPlacement::Top => (centered_x, top_y),
        UiDropdownPlacement::TopEnd => (end_x, top_y),
        UiDropdownPlacement::RightStart => (
            anchor_rect.left + anchor_rect.width + OVERLAY_ANCHOR_GAP,
            anchor_rect.top,
        ),
        UiDropdownPlacement::LeftStart => (
            anchor_rect.left - dropdown_width - OVERLAY_ANCHOR_GAP,
            anchor_rect.top,
        ),
    }
}

#[cfg(test)]
fn dropdown_overflow_score(
    x: f64,
    y: f64,
    dropdown_width: f64,
    dropdown_height: f64,
    viewport_width: f64,
    viewport_height: f64,
) -> f64 {
    let left_overflow = (0.0 - x).max(0.0);
    let top_overflow = (0.0 - y).max(0.0);
    let right_overflow = (x + dropdown_width - viewport_width).max(0.0);
    let bottom_overflow = (y + dropdown_height - viewport_height).max(0.0);

    left_overflow + top_overflow + right_overflow + bottom_overflow
}

#[cfg(test)]
fn clamp_dropdown_origin(
    x: f64,
    y: f64,
    dropdown_width: f64,
    dropdown_height: f64,
    viewport_width: f64,
    viewport_height: f64,
) -> (f64, f64) {
    let max_x = (viewport_width - dropdown_width).max(0.0);
    let max_y = (viewport_height - dropdown_height).max(0.0);
    (x.clamp(0.0, max_x), y.clamp(0.0, max_y))
}

#[cfg(test)]
fn dropdown_auto_flip_order(preferred: UiDropdownPlacement) -> [UiDropdownPlacement; 8] {
    match preferred {
        UiDropdownPlacement::Center => [
            UiDropdownPlacement::Center,
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::Top,
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::BottomEnd,
            UiDropdownPlacement::TopEnd,
            UiDropdownPlacement::RightStart,
        ],
        UiDropdownPlacement::Left => [
            UiDropdownPlacement::Left,
            UiDropdownPlacement::Right,
            UiDropdownPlacement::LeftStart,
            UiDropdownPlacement::RightStart,
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::Top,
        ],
        UiDropdownPlacement::Right => [
            UiDropdownPlacement::Right,
            UiDropdownPlacement::Left,
            UiDropdownPlacement::RightStart,
            UiDropdownPlacement::LeftStart,
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::Top,
        ],
        UiDropdownPlacement::BottomStart => [
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::BottomEnd,
            UiDropdownPlacement::TopEnd,
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::Top,
            UiDropdownPlacement::RightStart,
            UiDropdownPlacement::LeftStart,
        ],
        UiDropdownPlacement::Bottom => [
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::Top,
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::BottomEnd,
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::TopEnd,
            UiDropdownPlacement::RightStart,
            UiDropdownPlacement::LeftStart,
        ],
        UiDropdownPlacement::BottomEnd => [
            UiDropdownPlacement::BottomEnd,
            UiDropdownPlacement::TopEnd,
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::Top,
            UiDropdownPlacement::RightStart,
            UiDropdownPlacement::LeftStart,
        ],
        UiDropdownPlacement::TopStart => [
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::TopEnd,
            UiDropdownPlacement::BottomEnd,
            UiDropdownPlacement::Top,
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::RightStart,
            UiDropdownPlacement::LeftStart,
        ],
        UiDropdownPlacement::Top => [
            UiDropdownPlacement::Top,
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::TopEnd,
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::BottomEnd,
            UiDropdownPlacement::RightStart,
            UiDropdownPlacement::LeftStart,
        ],
        UiDropdownPlacement::TopEnd => [
            UiDropdownPlacement::TopEnd,
            UiDropdownPlacement::BottomEnd,
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::Top,
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::RightStart,
            UiDropdownPlacement::LeftStart,
        ],
        UiDropdownPlacement::RightStart => [
            UiDropdownPlacement::RightStart,
            UiDropdownPlacement::LeftStart,
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::Top,
            UiDropdownPlacement::BottomEnd,
            UiDropdownPlacement::TopEnd,
        ],
        UiDropdownPlacement::LeftStart => [
            UiDropdownPlacement::LeftStart,
            UiDropdownPlacement::RightStart,
            UiDropdownPlacement::BottomStart,
            UiDropdownPlacement::TopStart,
            UiDropdownPlacement::Bottom,
            UiDropdownPlacement::Top,
            UiDropdownPlacement::BottomEnd,
            UiDropdownPlacement::TopEnd,
        ],
    }
}

#[cfg(test)]
fn select_dropdown_origin(
    anchor_rect: OverlayAnchorRect,
    dropdown_width: f64,
    dropdown_height: f64,
    viewport_width: f64,
    viewport_height: f64,
    preferred_placement: UiDropdownPlacement,
    auto_flip: bool,
) -> (UiDropdownPlacement, f64, f64) {
    let order = dropdown_auto_flip_order(preferred_placement);

    if !auto_flip {
        let (x, y) = dropdown_origin_for_placement(
            anchor_rect,
            dropdown_width,
            dropdown_height,
            preferred_placement,
        );
        let (x, y) = clamp_dropdown_origin(
            x,
            y,
            dropdown_width,
            dropdown_height,
            viewport_width,
            viewport_height,
        );
        return (preferred_placement, x, y);
    }

    let mut best = None;

    for placement in order {
        let (x, y) =
            dropdown_origin_for_placement(anchor_rect, dropdown_width, dropdown_height, placement);
        let overflow = dropdown_overflow_score(
            x,
            y,
            dropdown_width,
            dropdown_height,
            viewport_width,
            viewport_height,
        );

        if overflow <= f64::EPSILON {
            let (x, y) = clamp_dropdown_origin(
                x,
                y,
                dropdown_width,
                dropdown_height,
                viewport_width,
                viewport_height,
            );
            return (placement, x, y);
        }

        match best {
            None => best = Some((placement, overflow, x, y)),
            Some((_, best_overflow, _, _)) if overflow < best_overflow => {
                best = Some((placement, overflow, x, y));
            }
            _ => {}
        }
    }

    let (placement, _overflow, x, y) = best.unwrap_or({
        let (x, y) = dropdown_origin_for_placement(
            anchor_rect,
            dropdown_width,
            dropdown_height,
            preferred_placement,
        );
        (preferred_placement, f64::INFINITY, x, y)
    });

    let (x, y) = clamp_dropdown_origin(
        x,
        y,
        dropdown_width,
        dropdown_height,
        viewport_width,
        viewport_height,
    );
    (placement, x, y)
}

fn project_overlay_root(_: &UiOverlayRoot, ctx: ProjectionCtx<'_>) -> UiView {
    Arc::new(
        zstack(ctx.children)
            .alignment(UnitPoint::TOP_LEFT)
            .width(Dim::Stretch)
            .height(Dim::Stretch),
    )
}

fn app_i18n_font_stack(world: &World) -> Option<Vec<String>> {
    world
        .get_resource::<AppI18n>()
        .map(AppI18n::get_font_stack)
        .filter(|stack| !stack.is_empty())
}

fn localized_font_stack(world: &World, entity: Entity) -> Option<Vec<String>> {
    if world.get::<LocalizeText>(entity).is_none() {
        return None;
    }

    app_i18n_font_stack(world)
}

fn project_label(label_component: &UiLabel, ctx: ProjectionCtx<'_>) -> UiView {
    let mut style = resolve_style(ctx.world, ctx.entity);
    let text = resolve_localized_text(ctx.world, ctx.entity, &label_component.text);
    if let Some(stack) = localized_font_stack(ctx.world, ctx.entity) {
        style.font_family = Some(stack);
    }
    let localization_key = ctx
        .world
        .get::<LocalizeText>(ctx.entity)
        .map(|localize| localize.key.as_str());
    trace!(
        entity = ?ctx.entity,
        localization_key = ?localization_key,
        fallback_text = %label_component.text,
        resolved_text = %text,
        "projected UiLabel text"
    );
    Arc::new(apply_label_style(label(text), &style))
}

fn project_button(button_component: &UiButton, ctx: ProjectionCtx<'_>) -> UiView {
    let mut style = resolve_style(ctx.world, ctx.entity);
    let button_label_text = resolve_localized_text(ctx.world, ctx.entity, &button_component.label);
    if let Some(stack) = localized_font_stack(ctx.world, ctx.entity) {
        style.font_family = Some(stack);
    }
    let localization_key = ctx
        .world
        .get::<LocalizeText>(ctx.entity)
        .map(|localize| localize.key.as_str());
    trace!(
        entity = ?ctx.entity,
        localization_key = ?localization_key,
        fallback_text = %button_component.label,
        resolved_text = %button_label_text,
        "projected UiButton label"
    );

    let label_child = apply_label_style(label(button_label_text), &style);

    Arc::new(apply_direct_widget_style(
        ecs_button_with_child(ctx.entity, BuiltinUiAction::Clicked, label_child),
        &style,
    ))
}

fn project_dialog(dialog: &UiDialog, ctx: ProjectionCtx<'_>) -> UiView {
    let mut dialog_style = resolve_style(ctx.world, ctx.entity);
    if dialog_style.colors.bg.is_none() {
        dialog_style.colors.bg = Some(xilem::Color::from_rgb8(0x18, 0x1E, 0x2D));
    }
    if dialog_style.colors.border.is_none() {
        dialog_style.colors.border = Some(xilem::Color::from_rgb8(0x3A, 0x48, 0x68));
    }
    if dialog_style.layout.padding <= 0.0 {
        dialog_style.layout.padding = 18.0;
    }
    if dialog_style.layout.corner_radius <= 0.0 {
        dialog_style.layout.corner_radius = 12.0;
    }
    if dialog_style.layout.border_width <= 0.0 {
        dialog_style.layout.border_width = 1.0;
    }
    if dialog_style.box_shadow.is_none() {
        dialog_style.box_shadow =
            Some(BoxShadow::new(BLACK.with_alpha(0.36), (0.0, 10.0)).blur(22.0));
    }

    let mut backdrop_style = resolve_style_for_classes(ctx.world, ["overlay.dialog.backdrop"]);
    if backdrop_style.colors.bg.is_none() {
        backdrop_style.colors.bg = Some(xilem::Color::from_rgba8(0, 0, 0, 160));
    }

    let mut title_style = resolve_style_for_classes(ctx.world, ["overlay.dialog.title"]);
    let mut body_style = resolve_style_for_classes(ctx.world, ["overlay.dialog.body"]);
    let mut dismiss_style = resolve_style_for_classes(ctx.world, ["overlay.dialog.dismiss"]);
    if dismiss_style.layout.padding <= 0.0 {
        dismiss_style.layout.padding = 8.0;
    }

    let title = translate_text(ctx.world, dialog.title_key.as_deref(), &dialog.title);
    let body = translate_text(ctx.world, dialog.body_key.as_deref(), &dialog.body);
    let dismiss_label = translate_text(
        ctx.world,
        dialog.dismiss_key.as_deref(),
        &dialog.dismiss_label,
    );

    if (dialog.title_key.is_some() || dialog.body_key.is_some() || dialog.dismiss_key.is_some())
        && let Some(stack) = app_i18n_font_stack(ctx.world)
    {
        title_style.font_family = Some(stack.clone());
        body_style.font_family = Some(stack.clone());
        dismiss_style.font_family = Some(stack);
    }

    let computed_position = ctx
        .world
        .get::<OverlayComputedPosition>(ctx.entity)
        .copied()
        .unwrap_or_default();

    let estimated_width = estimate_dialog_surface_width_px(
        &title,
        &body,
        &dismiss_label,
        title_style.text.size,
        body_style.text.size,
        dismiss_style.text.size,
        dialog_style.layout.padding.max(12.0),
    );

    let dialog_gap = dialog_style.layout.gap.max(10.0);
    let estimated_height = estimate_dialog_surface_height_px(
        &title,
        &body,
        estimated_width,
        title_style.text.size,
        body_style.text.size,
        dismiss_style.text.size,
        dismiss_style.layout.padding.max(8.0),
        dialog_gap,
        dialog_style.layout.padding.max(12.0),
        dialog_style.layout.padding.max(12.0),
    );

    let dialog_surface_width = if computed_position.width > 1.0 {
        computed_position.width
    } else {
        estimated_width
    };

    let dialog_surface_height = if computed_position.height > 1.0 {
        computed_position.height
    } else {
        estimated_height
    };

    let backdrop = apply_direct_widget_style(
        ecs_button(ctx.entity, OverlayUiAction::DismissDialog, "")
            .width(Dim::Stretch)
            .height(Dim::Stretch),
        &backdrop_style,
    );

    let dialog_surface = xilem_masonry::view::sized_box(apply_widget_style(
        flex_col(vec![
            apply_label_style(label(title), &title_style).into_any_flex(),
            apply_label_style(label(body), &body_style).into_any_flex(),
            apply_direct_widget_style(
                ecs_button(ctx.entity, OverlayUiAction::DismissDialog, dismiss_label),
                &dismiss_style,
            )
            .into_any_flex(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(dialog_gap)),
        &dialog_style,
    ))
    .fixed_width(Length::px(dialog_surface_width))
    .fixed_height(Length::px(dialog_surface_height));

    let positioned_surface =
        transformed(dialog_surface).translate((computed_position.x, computed_position.y));

    Arc::new(
        zstack((backdrop, positioned_surface.alignment(UnitPoint::TOP_LEFT)))
            .alignment(UnitPoint::TOP_LEFT)
            .width(Dim::Stretch)
            .height(Dim::Stretch),
    )
}

fn project_combo_box(combo_box: &UiComboBox, ctx: ProjectionCtx<'_>) -> UiView {
    let mut style = resolve_style(ctx.world, ctx.entity);

    if (combo_box.placeholder_key.is_some()
        || combo_box
            .options
            .iter()
            .any(|option| option.label_key.is_some()))
        && let Some(stack) = app_i18n_font_stack(ctx.world)
    {
        style.font_family = Some(stack);
    }

    let selected_label = combo_box
        .clamped_selected()
        .and_then(|idx| combo_box.options.get(idx))
        .map(|option| translate_text(ctx.world, option.label_key.as_deref(), &option.label))
        .unwrap_or_else(|| {
            translate_text(
                ctx.world,
                combo_box.placeholder_key.as_deref(),
                &combo_box.placeholder,
            )
        });

    let arrow = if combo_box.is_open { "▴" } else { "▾" };
    let button_label = format!("{selected_label}  {arrow}");

    Arc::new(apply_direct_widget_style(
        ecs_button(ctx.entity, OverlayUiAction::ToggleCombo, button_label),
        &style,
    ))
}

fn project_dropdown_menu(_: &UiDropdownMenu, ctx: ProjectionCtx<'_>) -> UiView {
    let anchor = ctx
        .world
        .get::<AnchoredTo>(ctx.entity)
        .map(|anchored| anchored.0);

    let mut menu_style = resolve_style_for_classes(ctx.world, ["overlay.dropdown.menu"]);
    if menu_style.colors.bg.is_none() {
        menu_style.colors.bg = Some(xilem::Color::from_rgb8(0x16, 0x1C, 0x2A));
    }
    if menu_style.colors.border.is_none() {
        menu_style.colors.border = Some(xilem::Color::from_rgb8(0x38, 0x46, 0x64));
    }
    if menu_style.layout.padding <= 0.0 {
        menu_style.layout.padding = 8.0;
    }
    if menu_style.layout.corner_radius <= 0.0 {
        menu_style.layout.corner_radius = 10.0;
    }
    if menu_style.layout.border_width <= 0.0 {
        menu_style.layout.border_width = 1.0;
    }
    if menu_style.box_shadow.is_none() {
        menu_style.box_shadow = Some(BoxShadow::new(BLACK.with_alpha(0.28), (0.0, 8.0)).blur(16.0));
    }

    let mut item_style = resolve_style_for_classes(ctx.world, ["overlay.dropdown.item"]);

    let options_have_localized_labels = anchor
        .and_then(|anchor| ctx.world.get::<UiComboBox>(anchor))
        .is_some_and(|combo_box| {
            combo_box
                .options
                .iter()
                .any(|option| option.label_key.is_some())
        });

    if options_have_localized_labels && let Some(stack) = app_i18n_font_stack(ctx.world) {
        item_style.font_family = Some(stack);
    }

    let translated_options = anchor
        .and_then(|anchor| ctx.world.get::<UiComboBox>(anchor))
        .map(|combo_box| {
            combo_box
                .options
                .iter()
                .map(|option| translate_text(ctx.world, option.label_key.as_deref(), &option.label))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let anchor_rect = ctx
        .world
        .get::<OverlayAnchorRect>(ctx.entity)
        .copied()
        .unwrap_or_default();

    let computed_position = ctx
        .world
        .get::<OverlayComputedPosition>(ctx.entity)
        .copied()
        .unwrap_or_default();

    let estimated_dropdown_width = estimate_dropdown_surface_width_px(
        anchor_rect.width.max(1.0),
        translated_options.iter().map(String::as_str),
        item_style.text.size,
        item_style.layout.padding * 2.0 + menu_style.layout.padding * 2.0,
    );

    let item_gap = menu_style.layout.gap.max(6.0);
    let estimated_dropdown_height = estimate_dropdown_viewport_height_px(
        translated_options.len(),
        item_style.text.size,
        item_style.layout.padding,
        item_gap,
    );

    let dropdown_width = if computed_position.width > 1.0 {
        computed_position.width
    } else {
        estimated_dropdown_width
    };

    let dropdown_height = if computed_position.height > 1.0 {
        computed_position.height
    } else {
        estimated_dropdown_height
    };

    let dropdown_x = computed_position.x;
    let dropdown_y = computed_position.y;

    let items = translated_options
        .into_iter()
        .enumerate()
        .map(|(index, label_text)| {
            let item_button = ecs_button(
                ctx.entity,
                OverlayUiAction::SelectComboItem { index },
                label_text,
            )
            .width(Dim::Stretch);

            apply_direct_widget_style(item_button, &item_style).into_any_flex()
        })
        .collect::<Vec<_>>();

    let scrollable_menu = portal(
        flex_col(items)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .width(Dim::Stretch)
            .gap(Length::px(item_gap)),
    )
    .dims((Length::px(dropdown_width), Length::px(dropdown_height)));

    let dropdown_panel = transformed(apply_widget_style(scrollable_menu, &menu_style))
        .translate((dropdown_x, dropdown_y));

    Arc::new(
        zstack((dropdown_panel.alignment(UnitPoint::TOP_LEFT),))
            .alignment(UnitPoint::TOP_LEFT)
            .width(Dim::Stretch)
            .height(Dim::Stretch),
    )
}

/// Register built-in projectors for built-in ECS demo components.
pub fn register_builtin_projectors(registry: &mut UiProjectorRegistry) {
    registry
        .register_component::<UiFlexColumn>(project_flex_column)
        .register_component::<UiFlexRow>(project_flex_row)
        .register_component::<UiLabel>(project_label)
        .register_component::<UiButton>(project_button)
        .register_component::<UiOverlayRoot>(project_overlay_root)
        .register_component::<UiDialog>(project_dialog)
        .register_component::<UiComboBox>(project_combo_box)
        .register_component::<UiDropdownMenu>(project_dropdown_menu);
}

#[cfg(test)]
mod tests {
    use super::{
        DIALOG_SURFACE_MAX_WIDTH, DIALOG_SURFACE_MIN_WIDTH, DROPDOWN_MAX_VIEWPORT_HEIGHT,
        OverlayAnchorRect, UiDropdownPlacement, estimate_dialog_surface_width_px,
        estimate_dropdown_surface_width_px, estimate_dropdown_viewport_height_px,
        select_dropdown_origin,
    };

    #[test]
    fn dialog_surface_width_estimation_is_clamped() {
        let width = estimate_dialog_surface_width_px(
            "Very long modal title that should hit max width",
            "This is a long body line that should also be measured for width and then clamped.",
            "Close",
            24.0,
            16.0,
            15.0,
            16.0,
        );

        assert!((DIALOG_SURFACE_MIN_WIDTH..=DIALOG_SURFACE_MAX_WIDTH).contains(&width));
        assert_eq!(
            estimate_dialog_surface_width_px("", "", "", 24.0, 16.0, 15.0, 16.0),
            DIALOG_SURFACE_MIN_WIDTH
        );
    }

    #[test]
    fn dropdown_width_estimation_respects_anchor_min_width() {
        let width = estimate_dropdown_surface_width_px(180.0, ["One", "Two", "Three"], 16.0, 24.0);
        assert!(width >= 180.0);

        let wide = estimate_dropdown_surface_width_px(
            120.0,
            ["An exceptionally long option label that should grow the menu"],
            16.0,
            24.0,
        );
        assert!(wide > 120.0);
    }

    #[test]
    fn dropdown_viewport_height_is_capped() {
        let height = estimate_dropdown_viewport_height_px(40, 16.0, 10.0, 6.0);
        assert_eq!(height, DROPDOWN_MAX_VIEWPORT_HEIGHT);

        let small = estimate_dropdown_viewport_height_px(2, 16.0, 10.0, 6.0);
        assert!(small < DROPDOWN_MAX_VIEWPORT_HEIGHT);
        assert!(small > 0.0);
    }

    #[test]
    fn dropdown_auto_flips_to_top_when_bottom_has_no_space() {
        let anchor = OverlayAnchorRect {
            left: 24.0,
            top: 168.0,
            width: 160.0,
            height: 32.0,
        };

        let (placement, _x, y) = select_dropdown_origin(
            anchor,
            200.0,
            120.0,
            360.0,
            220.0,
            UiDropdownPlacement::BottomStart,
            true,
        );

        assert_eq!(placement, UiDropdownPlacement::TopStart);
        assert!(y < anchor.top);
    }

    #[test]
    fn dropdown_respects_fixed_placement_when_auto_flip_disabled() {
        let anchor = OverlayAnchorRect {
            left: 250.0,
            top: 64.0,
            width: 80.0,
            height: 28.0,
        };

        let (placement, x, _y) = select_dropdown_origin(
            anchor,
            180.0,
            100.0,
            300.0,
            200.0,
            UiDropdownPlacement::RightStart,
            false,
        );

        assert_eq!(placement, UiDropdownPlacement::RightStart);
        assert!(x <= 300.0 - 180.0);
    }

    #[test]
    fn dropdown_auto_flips_to_left_for_right_edge_anchor() {
        let anchor = OverlayAnchorRect {
            left: 282.0,
            top: 40.0,
            width: 24.0,
            height: 24.0,
        };

        let (placement, _x, _y) = select_dropdown_origin(
            anchor,
            140.0,
            120.0,
            320.0,
            240.0,
            UiDropdownPlacement::RightStart,
            true,
        );

        assert_eq!(placement, UiDropdownPlacement::LeftStart);
    }
}
