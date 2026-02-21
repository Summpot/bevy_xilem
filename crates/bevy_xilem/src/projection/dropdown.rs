use crate::{
    ecs::{AnchoredTo, OverlayAnchorRect, OverlayComputedPosition, UiComboBox, UiDropdownMenu},
    overlay::OverlayUiAction,
    styling::{
        apply_direct_widget_style, apply_label_style, apply_widget_style, resolve_style,
        resolve_style_for_classes,
    },
    views::{ecs_button, ecs_button_with_child, opaque_hitbox_for_entity},
};
use masonry::layout::{Dim, Length};
use std::sync::Arc;
use xilem::{palette::css::BLACK, style::BoxShadow, style::Style as _};
use xilem_masonry::view::{CrossAxisAlignment, FlexExt as _, flex_col, label, portal, transformed};

#[cfg(test)]
use crate::UiDropdownPlacement;

use super::{
    core::{ProjectionCtx, UiView},
    utils::{
        app_i18n_font_stack, estimate_text_width_px, hide_style_without_collapsing_layout,
        translate_text,
    },
};

pub(crate) const DROPDOWN_MAX_VIEWPORT_HEIGHT: f64 = 300.0;
#[cfg(test)]
pub(crate) const OVERLAY_ANCHOR_GAP: f64 = 4.0;

pub(crate) fn estimate_dropdown_surface_width_px<'a>(
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

pub(crate) fn estimate_dropdown_viewport_height_px(
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
pub(crate) fn dropdown_origin_for_placement(
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
pub(crate) fn dropdown_overflow_score(
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
pub(crate) fn clamp_dropdown_origin(
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
pub(crate) fn dropdown_auto_flip_order(preferred: UiDropdownPlacement) -> [UiDropdownPlacement; 8] {
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
pub(crate) fn select_dropdown_origin(
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

pub(crate) fn project_combo_box(combo_box: &UiComboBox, ctx: ProjectionCtx<'_>) -> UiView {
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

pub(crate) fn project_dropdown_menu(_: &UiDropdownMenu, ctx: ProjectionCtx<'_>) -> UiView {
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

    let anchor_width = ctx
        .world
        .get::<OverlayAnchorRect>(ctx.entity)
        .map(|anchor_rect| anchor_rect.width)
        .unwrap_or(160.0);

    let computed_position = ctx
        .world
        .get::<OverlayComputedPosition>(ctx.entity)
        .copied()
        .unwrap_or_default();

    if !computed_position.is_positioned {
        hide_style_without_collapsing_layout(&mut menu_style);
        hide_style_without_collapsing_layout(&mut item_style);
    }

    let estimated_dropdown_width = estimate_dropdown_surface_width_px(
        anchor_width.max(1.0),
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
            let item_button = ecs_button_with_child(
                ctx.entity,
                OverlayUiAction::SelectComboItem { index },
                apply_label_style(label(label_text), &item_style),
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

    let dropdown_panel = transformed(opaque_hitbox_for_entity(
        ctx.entity,
        apply_widget_style(scrollable_menu, &menu_style),
    ))
    .translate((dropdown_x, dropdown_y));

    Arc::new(dropdown_panel)
}

#[cfg(test)]
mod tests {
    use super::{
        DROPDOWN_MAX_VIEWPORT_HEIGHT, OverlayAnchorRect, UiDropdownPlacement,
        estimate_dropdown_surface_width_px, estimate_dropdown_viewport_height_px,
        select_dropdown_origin,
    };

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
