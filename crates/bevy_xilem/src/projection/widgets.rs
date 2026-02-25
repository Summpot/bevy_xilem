use std::sync::Arc;

use bevy_ecs::{
    entity::Entity,
    hierarchy::{ChildOf, Children},
    prelude::Component,
};
use masonry::kurbo::{Axis, Point};
use masonry::layout::{Dim, Length};
use xilem::Color;
use xilem::style::Style as _;
use xilem_masonry::view::{
    FlexExt as _, flex_col, flex_row, label, sized_box, spinner, split, transformed, zstack,
};

use crate::{
    ecs::{
        AnchoredTo, OverlayComputedPosition, PartScrollBarHorizontal, PartScrollBarVertical,
        PartScrollThumbHorizontal, PartScrollThumbVertical, PartScrollViewport, ScrollAxis,
        SplitDirection, ToastKind, UiColorPicker, UiColorPickerPanel, UiDatePicker,
        UiDatePickerPanel, UiGroupBox, UiMenuBar, UiMenuBarItem, UiMenuItemPanel, UiRadioGroup,
        UiScrollView, UiSpinner, UiSplitPane, UiTabBar, UiTable, UiToast, UiTooltip, UiTreeNode,
    },
    overlay::OverlayUiAction,
    styling::{
        ResolvedStyle, apply_direct_widget_style, apply_flex_alignment, apply_label_style,
        apply_widget_style, resolve_style, resolve_style_for_classes,
    },
    views::{
        ecs_button, ecs_button_with_child, ecs_drag_thumb, opaque_hitbox_for_entity, scroll_portal,
    },
    widget_actions::WidgetUiAction,
};

use super::core::{ProjectionCtx, UiView};
use super::utils::{VectorIcon, hide_style_without_collapsing_layout, vector_icon};

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let is_leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if is_leap { 29 } else { 28 }
        }
        _ => 30,
    }
}

/// Returns weekday of the 1st of the month. 0=Sun, 1=Mon, …, 6=Sat.
fn day_of_week_for_first(year: i32, month: u32) -> u32 {
    let t: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut y = year;
    if month < 3 {
        y -= 1;
    }
    let y = y as u32;
    (y + y / 4 - y / 100 + y / 400 + t[(month - 1) as usize] + 1) % 7
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "——",
    }
}

fn tree_node_depth(world: &bevy_ecs::world::World, entity: bevy_ecs::entity::Entity) -> u32 {
    let mut depth = 0u32;
    let mut current = entity;
    while let Some(child_of) = world.get::<ChildOf>(current) {
        current = child_of.parent();
        if world.get::<UiTreeNode>(current).is_some() {
            depth += 1;
        }
    }
    depth
}

/// Build a default dark panel style used by overlay panels.
fn default_panel_style(world: &bevy_ecs::world::World, class: &str) -> ResolvedStyle {
    let mut style = resolve_style_for_classes(world, [class]);
    if style.colors.bg.is_none() {
        style.colors.bg = Some(Color::from_rgb8(0x1F, 0x1F, 0x1F));
    }
    if style.colors.border.is_none() {
        style.colors.border = Some(Color::from_rgb8(0x3F, 0x3F, 0x3F));
    }
    if style.layout.padding <= 0.0 {
        style.layout.padding = 8.0;
    }
    if style.layout.corner_radius <= 0.0 {
        style.layout.corner_radius = 8.0;
    }
    if style.layout.border_width <= 0.0 {
        style.layout.border_width = 1.0;
    }
    style
}

fn default_item_style(world: &bevy_ecs::world::World, class: &str) -> ResolvedStyle {
    let mut style = resolve_style_for_classes(world, [class]);
    if style.layout.padding <= 0.0 {
        style.layout.padding = 6.0;
    }
    style
}

/// Returns the entity and (x, y) of a positioned overlay, or None if not yet positioned.
fn overlay_position(
    world: &bevy_ecs::world::World,
    entity: bevy_ecs::entity::Entity,
) -> Option<(f64, f64)> {
    let pos = world.get::<OverlayComputedPosition>(entity)?;
    if pos.is_positioned {
        Some((pos.x, pos.y))
    } else {
        None
    }
}

/// Create a hidden empty placeholder when an overlay isn't positioned yet.
fn hidden_placeholder() -> UiView {
    Arc::new(
        sized_box(label(""))
            .width(Dim::Fixed(Length::px(0.0)))
            .height(Dim::Fixed(Length::px(0.0))),
    )
}

fn child_entity_views(ctx: &ProjectionCtx<'_>) -> Vec<(Entity, UiView)> {
    let child_entities = ctx
        .world
        .get::<Children>(ctx.entity)
        .map(|children| children.iter().copied().collect::<Vec<_>>())
        .unwrap_or_default();

    child_entities
        .into_iter()
        .zip(ctx.children.iter().cloned())
        .collect::<Vec<_>>()
}

fn first_part_entity<P: Component>(
    ctx: &ProjectionCtx<'_>,
    pairs: &[(Entity, UiView)],
) -> Option<Entity> {
    pairs
        .iter()
        .find_map(|(entity, _)| ctx.world.get::<P>(*entity).map(|_| *entity))
}

const SCROLLBAR_THICKNESS: f64 = 12.0;
const SCROLLBAR_MIN_THUMB: f64 = 24.0;

fn thumb_length(viewport: f64, content: f64) -> f64 {
    if content <= 0.0 {
        return viewport.max(0.0);
    }
    let ratio = (viewport / content).clamp(0.0, 1.0);
    (viewport * ratio).clamp(SCROLLBAR_MIN_THUMB.min(viewport), viewport)
}

fn thumb_offset(current_offset: f64, max_offset: f64, track_len: f64, thumb_len: f64) -> f64 {
    let travel = (track_len - thumb_len).max(0.0);
    if max_offset <= f64::EPSILON {
        0.0
    } else {
        (current_offset / max_offset).clamp(0.0, 1.0) * travel
    }
}

pub(crate) fn project_scroll_view(scroll_view: &UiScrollView, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let pairs = child_entity_views(&ctx);

    let viewport_part = first_part_entity::<PartScrollViewport>(&ctx, &pairs);
    let vertical_track_part = first_part_entity::<PartScrollBarVertical>(&ctx, &pairs);
    let vertical_thumb_part = first_part_entity::<PartScrollThumbVertical>(&ctx, &pairs);
    let horizontal_track_part = first_part_entity::<PartScrollBarHorizontal>(&ctx, &pairs);
    let horizontal_thumb_part = first_part_entity::<PartScrollThumbHorizontal>(&ctx, &pairs);

    let mut scroll_state = *scroll_view;
    scroll_state.clamp_scroll_offset();

    let viewport_w = (scroll_state.viewport_size.x as f64).max(32.0);
    let viewport_h = (scroll_state.viewport_size.y as f64).max(32.0);
    let content_w = (scroll_state.content_size.x as f64).max(viewport_w);
    let content_h = (scroll_state.content_size.y as f64).max(viewport_h);

    let content_views = pairs
        .iter()
        .filter_map(|(entity, view)| {
            let is_template_part = ctx.world.get::<PartScrollViewport>(*entity).is_some()
                || ctx.world.get::<PartScrollBarVertical>(*entity).is_some()
                || ctx.world.get::<PartScrollThumbVertical>(*entity).is_some()
                || ctx.world.get::<PartScrollBarHorizontal>(*entity).is_some()
                || ctx
                    .world
                    .get::<PartScrollThumbHorizontal>(*entity)
                    .is_some();
            (!is_template_part).then_some(view.clone().into_any_flex())
        })
        .collect::<Vec<_>>();

    let content_view = if content_views.is_empty() {
        flex_col(vec![label("").into_any_flex()])
    } else {
        flex_col(content_views)
    };

    let portal = scroll_portal(
        content_view,
        Point::new(
            scroll_state.scroll_offset.x as f64,
            scroll_state.scroll_offset.y as f64,
        ),
    )
    .constrain_horizontal(false)
    .constrain_vertical(false)
    .content_must_fill(true);

    let viewport_style = viewport_part
        .map(|entity| resolve_style(ctx.world, entity))
        .unwrap_or_else(|| resolve_style_for_classes(ctx.world, ["template.scroll_view.viewport"]));

    let viewport_surface = apply_widget_style(
        sized_box(portal)
            .width(Dim::Fixed(Length::px(viewport_w)))
            .height(Dim::Fixed(Length::px(viewport_h))),
        &viewport_style,
    );

    let max_x = (content_w - viewport_w).max(0.0);
    let max_y = (content_h - viewport_h).max(0.0);

    let show_vertical = scroll_state.show_vertical_scrollbar && max_y > f64::EPSILON;
    let show_horizontal = scroll_state.show_horizontal_scrollbar && max_x > f64::EPSILON;

    let vertical_bar_view = if show_vertical {
        let track_style = vertical_track_part
            .map(|entity| resolve_style(ctx.world, entity))
            .unwrap_or_else(|| {
                resolve_style_for_classes(ctx.world, ["template.scroll_view.scrollbar.vertical"])
            });
        let thumb_style = vertical_thumb_part
            .map(|entity| resolve_style(ctx.world, entity))
            .unwrap_or_else(|| {
                resolve_style_for_classes(ctx.world, ["template.scroll_view.thumb.vertical"])
            });

        let track_len = viewport_h;
        let thumb_len = thumb_length(viewport_h, content_h);
        let thumb_y = thumb_offset(
            scroll_state.scroll_offset.y as f64,
            max_y,
            track_len,
            thumb_len,
        );

        let track = apply_widget_style(
            sized_box(label(""))
                .width(Dim::Fixed(Length::px(SCROLLBAR_THICKNESS)))
                .height(Dim::Fixed(Length::px(track_len))),
            &track_style,
        );

        let thumb_body = if let Some(thumb_entity) = vertical_thumb_part {
            ecs_drag_thumb(thumb_entity, ScrollAxis::Vertical, "")
        } else {
            ecs_drag_thumb(ctx.entity, ScrollAxis::Vertical, "")
        };

        let thumb = apply_widget_style(
            sized_box(thumb_body)
                .width(Dim::Fixed(Length::px(SCROLLBAR_THICKNESS)))
                .height(Dim::Fixed(Length::px(thumb_len))),
            &thumb_style,
        );

        Some(zstack((track, transformed(thumb).translate((0.0, thumb_y)))).into_any_flex())
    } else {
        None
    };

    let horizontal_bar_view = if show_horizontal {
        let track_style = horizontal_track_part
            .map(|entity| resolve_style(ctx.world, entity))
            .unwrap_or_else(|| {
                resolve_style_for_classes(ctx.world, ["template.scroll_view.scrollbar.horizontal"])
            });
        let thumb_style = horizontal_thumb_part
            .map(|entity| resolve_style(ctx.world, entity))
            .unwrap_or_else(|| {
                resolve_style_for_classes(ctx.world, ["template.scroll_view.thumb.horizontal"])
            });

        let track_len = viewport_w;
        let thumb_len = thumb_length(viewport_w, content_w);
        let thumb_x = thumb_offset(
            scroll_state.scroll_offset.x as f64,
            max_x,
            track_len,
            thumb_len,
        );

        let track = apply_widget_style(
            sized_box(label(""))
                .width(Dim::Fixed(Length::px(track_len)))
                .height(Dim::Fixed(Length::px(SCROLLBAR_THICKNESS))),
            &track_style,
        );

        let thumb_body = if let Some(thumb_entity) = horizontal_thumb_part {
            ecs_drag_thumb(thumb_entity, ScrollAxis::Horizontal, "")
        } else {
            ecs_drag_thumb(ctx.entity, ScrollAxis::Horizontal, "")
        };

        let thumb = apply_widget_style(
            sized_box(thumb_body)
                .width(Dim::Fixed(Length::px(thumb_len)))
                .height(Dim::Fixed(Length::px(SCROLLBAR_THICKNESS))),
            &thumb_style,
        );

        Some(zstack((track, transformed(thumb).translate((thumb_x, 0.0)))).into_any_flex())
    } else {
        None
    };

    let mut top_row = vec![viewport_surface.into_any_flex()];
    if let Some(vertical_bar) = vertical_bar_view {
        top_row.push(vertical_bar);
    }

    let mut rows = vec![flex_row(top_row).gap(Length::px(0.0)).into_any_flex()];

    if let Some(horizontal_bar) = horizontal_bar_view {
        let mut bottom_row = vec![horizontal_bar];
        if show_vertical {
            bottom_row.push(
                sized_box(label(""))
                    .width(Dim::Fixed(Length::px(SCROLLBAR_THICKNESS)))
                    .height(Dim::Fixed(Length::px(SCROLLBAR_THICKNESS)))
                    .into_any_flex(),
            );
        }
        rows.push(flex_row(bottom_row).gap(Length::px(0.0)).into_any_flex());
    }

    Arc::new(apply_widget_style(
        apply_flex_alignment(flex_col(rows), &style).gap(Length::px(0.0)),
        &style,
    ))
}

// ---------------------------------------------------------------------------
// Radio Group
// ---------------------------------------------------------------------------

pub(crate) fn project_radio_group(radio_group: &UiRadioGroup, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let mut item_style = resolve_style_for_classes(ctx.world, ["widget.radio.item"]);
    if item_style.layout.padding <= 0.0 {
        item_style.layout.padding = 4.0;
    }

    let items = radio_group
        .options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let icon_color = item_style
                .colors
                .text
                .or(style.colors.text)
                .unwrap_or(Color::from_rgb8(0xE7, 0xEC, 0xF8));
            let indicator = if i == radio_group.selected {
                vector_icon(VectorIcon::RadioOn, 14.0, icon_color)
            } else {
                vector_icon(VectorIcon::RadioOff, 14.0, icon_color)
            };

            let content = flex_row(vec![
                indicator.into_any_flex(),
                apply_label_style(label(opt.clone()), &item_style).into_any_flex(),
            ])
            .gap(Length::px(6.0));

            let btn = ecs_button_with_child(
                ctx.entity,
                WidgetUiAction::SelectRadioItem {
                    group: ctx.entity,
                    index: i,
                },
                content,
            );
            apply_direct_widget_style(btn, &item_style).into_any_flex()
        })
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        apply_flex_alignment(flex_col(items), &style).gap(Length::px(style.layout.gap.max(4.0))),
        &style,
    ))
}

// ---------------------------------------------------------------------------
// Tab Bar
// ---------------------------------------------------------------------------

pub(crate) fn project_tab_bar(tab_bar: &UiTabBar, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let mut header_style = resolve_style_for_classes(ctx.world, ["widget.tab.header"]);
    if header_style.layout.padding <= 0.0 {
        header_style.layout.padding = 8.0;
    }
    let mut active_style = resolve_style_for_classes(ctx.world, ["widget.tab.active"]);
    if active_style.layout.padding <= 0.0 {
        active_style.layout.padding = 8.0;
    }
    if active_style.colors.border.is_none() {
        active_style.colors.border = Some(Color::from_rgb8(0x00, 0x78, 0xD4));
    }
    if active_style.layout.border_width <= 0.0 {
        active_style.layout.border_width = 2.0;
    }

    let headers = tab_bar
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab_label)| {
            let s = if i == tab_bar.active {
                &active_style
            } else {
                &header_style
            };
            let btn = ecs_button(
                ctx.entity,
                WidgetUiAction::SelectTab {
                    bar: ctx.entity,
                    index: i,
                },
                tab_label.clone(),
            );
            apply_direct_widget_style(btn, s).into_any_flex()
        })
        .collect::<Vec<_>>();

    let header_row = flex_row(headers).into_any_flex();

    let content: UiView = ctx
        .children
        .get(tab_bar.active)
        .cloned()
        .unwrap_or_else(|| Arc::new(label("")));

    Arc::new(apply_widget_style(
        apply_flex_alignment(flex_col(vec![header_row, content.into_any_flex()]), &style)
            .gap(Length::px(0.0)),
        &style,
    ))
}

// ---------------------------------------------------------------------------
// Tree Node
// ---------------------------------------------------------------------------

pub(crate) fn project_tree_node(tree_node: &UiTreeNode, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let depth = tree_node_depth(ctx.world, ctx.entity);
    let indent = (depth as f64) * 16.0;

    let has_children = !ctx.children.is_empty();
    let icon_color = style
        .colors
        .text
        .unwrap_or(Color::from_rgb8(0xE7, 0xEC, 0xF8));

    let header: UiView = if has_children {
        let icon = if tree_node.is_expanded {
            vector_icon(VectorIcon::ChevronDown, 12.0, icon_color)
        } else {
            vector_icon(VectorIcon::ChevronRight, 12.0, icon_color)
        };
        let content = flex_row(vec![
            icon.into_any_flex(),
            apply_label_style(label(tree_node.label.clone()), &style).into_any_flex(),
        ])
        .gap(Length::px(6.0));

        let btn = ecs_button_with_child(
            ctx.entity,
            WidgetUiAction::ToggleTreeNode { node: ctx.entity },
            content,
        );
        Arc::new(apply_direct_widget_style(btn, &style))
    } else {
        Arc::new(apply_label_style(label(tree_node.label.clone()), &style))
    };

    let header_padded = sized_box(header).width(Dim::Stretch).into_any_flex();
    // We use padding / margin via a row with a spacer
    let spacer = sized_box(label(""))
        .width(Dim::Fixed(Length::px(indent)))
        .height(Dim::Fixed(Length::px(1.0)))
        .into_any_flex();
    let header_row = flex_row(vec![spacer, header_padded]);

    if tree_node.is_expanded && has_children {
        let children = ctx
            .children
            .into_iter()
            .map(|c| c.into_any_flex())
            .collect::<Vec<_>>();
        Arc::new(apply_widget_style(
            apply_flex_alignment(
                flex_col(vec![
                    header_row.into_any_flex(),
                    apply_flex_alignment(flex_col(children), &style).into_any_flex(),
                ]),
                &style,
            ),
            &style,
        ))
    } else {
        Arc::new(apply_widget_style(
            apply_flex_alignment(flex_col(vec![header_row.into_any_flex()]), &style),
            &style,
        ))
    }
}

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

pub(crate) fn project_table(table: &UiTable, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let mut header_style = resolve_style_for_classes(ctx.world, ["widget.table.header"]);
    if header_style.colors.bg.is_none() {
        header_style.colors.bg = Some(Color::from_rgb8(0x2A, 0x2A, 0x2A));
    }
    if header_style.layout.padding <= 0.0 {
        header_style.layout.padding = 6.0;
    }
    let mut cell_style = resolve_style_for_classes(ctx.world, ["widget.table.cell"]);
    if cell_style.layout.padding <= 0.0 {
        cell_style.layout.padding = 4.0;
    }

    // Header row
    let header_cells = table
        .columns
        .iter()
        .map(|col| {
            apply_widget_style(
                sized_box(apply_label_style(label(col.clone()), &header_style)).width(Dim::Stretch),
                &header_style,
            )
            .flex(1.0)
            .into_any_flex()
        })
        .collect::<Vec<_>>();
    let header_row = flex_row(header_cells).into_any_flex();

    // Data rows
    let data_rows = table
        .rows
        .iter()
        .enumerate()
        .map(|(row_idx, row)| {
            let mut row_style = cell_style.clone();
            if row_idx % 2 == 0 && row_style.colors.bg.is_none() {
                row_style.colors.bg = Some(Color::from_rgba8(255, 255, 255, 10));
            }
            let cells = row
                .iter()
                .map(|cell| {
                    apply_widget_style(
                        sized_box(apply_label_style(label(cell.clone()), &cell_style))
                            .width(Dim::Stretch),
                        &row_style,
                    )
                    .flex(1.0)
                    .into_any_flex()
                })
                .collect::<Vec<_>>();
            flex_row(cells).into_any_flex()
        })
        .collect::<Vec<_>>();

    let mut all_rows = vec![header_row];
    all_rows.extend(data_rows);

    Arc::new(apply_widget_style(
        apply_flex_alignment(flex_col(all_rows), &style).gap(Length::px(style.layout.gap.max(1.0))),
        &style,
    ))
}

// ---------------------------------------------------------------------------
// Menu Bar
// ---------------------------------------------------------------------------

pub(crate) fn project_menu_bar(_: &UiMenuBar, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|c| c.into_any_flex())
        .collect::<Vec<_>>();
    Arc::new(apply_widget_style(
        apply_flex_alignment(flex_row(children), &style).gap(Length::px(style.layout.gap.max(0.0))),
        &style,
    ))
}

pub(crate) fn project_menu_bar_item(item: &UiMenuBarItem, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let icon_color = style
        .colors
        .text
        .unwrap_or(Color::from_rgb8(0xE7, 0xEC, 0xF8));
    let icon = if item.is_open {
        vector_icon(VectorIcon::ChevronUp, 10.0, icon_color)
    } else {
        vector_icon(VectorIcon::ChevronDown, 10.0, icon_color)
    };
    let content = flex_row(vec![
        apply_label_style(label(item.label.clone()), &style).into_any_flex(),
        icon.into_any_flex(),
    ])
    .gap(Length::px(4.0));
    Arc::new(apply_direct_widget_style(
        ecs_button_with_child(ctx.entity, OverlayUiAction::ToggleMenuBarItem, content),
        &style,
    ))
}

pub(crate) fn project_menu_item_panel(_: &UiMenuItemPanel, ctx: ProjectionCtx<'_>) -> UiView {
    let anchor = ctx.world.get::<AnchoredTo>(ctx.entity).map(|a| a.0);

    let pos = match overlay_position(ctx.world, ctx.entity) {
        Some(p) => p,
        None => return hidden_placeholder(),
    };

    let menu_style = default_panel_style(ctx.world, "overlay.menu.panel");
    let item_style = default_item_style(ctx.world, "overlay.menu.item");

    let items: Vec<_> = anchor
        .and_then(|a| ctx.world.get::<UiMenuBarItem>(a))
        .map(|bar_item| {
            bar_item
                .items
                .iter()
                .enumerate()
                .map(|(i, menu_item)| {
                    apply_direct_widget_style(
                        ecs_button_with_child(
                            ctx.entity,
                            OverlayUiAction::SelectMenuBarItem { index: i },
                            apply_label_style(label(menu_item.label.clone()), &item_style),
                        ),
                        &item_style,
                    )
                    .into_any_flex()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let computed_pos = ctx
        .world
        .get::<OverlayComputedPosition>(ctx.entity)
        .copied()
        .unwrap_or_default();
    let panel_width = if computed_pos.width > 1.0 {
        computed_pos.width
    } else {
        160.0
    };
    let panel_height = if computed_pos.height > 1.0 {
        computed_pos.height
    } else {
        items.len() as f64 * 32.0 + 16.0
    };

    let panel_content = flex_col(items).gap(Length::px(menu_style.layout.gap.max(4.0)));

    let scrollable = crate::xilem::view::portal(panel_content)
        .dims((Length::px(panel_width), Length::px(panel_height)));

    Arc::new(
        transformed(crate::views::opaque_hitbox_for_entity(
            ctx.entity,
            apply_widget_style(scrollable, &menu_style),
        ))
        .translate(pos),
    )
}

// ---------------------------------------------------------------------------
// Tooltip
// ---------------------------------------------------------------------------

pub(crate) fn project_tooltip(tooltip: &UiTooltip, ctx: ProjectionCtx<'_>) -> UiView {
    let computed_pos = ctx
        .world
        .get::<OverlayComputedPosition>(ctx.entity)
        .copied()
        .unwrap_or_default();
    let pos = (computed_pos.x, computed_pos.y);

    let mut style = default_panel_style(ctx.world, "overlay.tooltip");
    if style.colors.bg.is_none() {
        style.colors.bg = Some(Color::from_rgb8(0x2B, 0x2B, 0x2B));
    }
    if style.colors.text.is_none() {
        style.colors.text = Some(Color::from_rgb8(0xF3, 0xF3, 0xF3));
    }
    if style.layout.padding <= 0.0 {
        style.layout.padding = 6.0;
    }
    if style.layout.corner_radius <= 0.0 {
        style.layout.corner_radius = 4.0;
    }

    if !computed_pos.is_positioned {
        hide_style_without_collapsing_layout(&mut style);
    }

    let text_lbl = apply_label_style(label(tooltip.text.clone()), &style);
    let panel = apply_widget_style(text_lbl, &style);

    Arc::new(transformed(opaque_hitbox_for_entity(ctx.entity, panel)).translate(pos))
}

// ---------------------------------------------------------------------------
// Spinner
// ---------------------------------------------------------------------------

pub(crate) fn project_spinner(sp: &UiSpinner, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let spin_view = spinner();

    if let Some(lbl) = &sp.label {
        let label_view = apply_label_style(label(lbl.clone()), &style);
        Arc::new(apply_widget_style(
            apply_flex_alignment(
                flex_row(vec![spin_view.into_any_flex(), label_view.into_any_flex()]),
                &style,
            )
            .gap(Length::px(8.0)),
            &style,
        ))
    } else {
        Arc::new(apply_widget_style(
            apply_flex_alignment(flex_row(vec![spin_view.into_any_flex()]), &style),
            &style,
        ))
    }
}

// ---------------------------------------------------------------------------
// Color Picker
// ---------------------------------------------------------------------------

const COLOR_SWATCHES: [(u8, u8, u8); 20] = [
    (255, 0, 0),
    (255, 128, 0),
    (255, 255, 0),
    (0, 255, 0),
    (0, 255, 128),
    (0, 255, 255),
    (0, 128, 255),
    (0, 0, 255),
    (128, 0, 255),
    (255, 0, 255),
    (255, 128, 128),
    (255, 200, 128),
    (255, 255, 128),
    (128, 255, 128),
    (128, 255, 200),
    (128, 200, 255),
    (128, 128, 255),
    (200, 128, 255),
    (255, 128, 200),
    (255, 255, 255),
];

pub(crate) fn project_color_picker(picker: &UiColorPicker, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let hex = format!("#{:02X}{:02X}{:02X}", picker.r, picker.g, picker.b);
    let icon_color = style
        .colors
        .text
        .unwrap_or(Color::from_rgb8(0xE7, 0xEC, 0xF8));
    let icon = if picker.is_open {
        vector_icon(VectorIcon::ChevronUp, 10.0, icon_color)
    } else {
        vector_icon(VectorIcon::ChevronDown, 10.0, icon_color)
    };
    let content = flex_row(vec![
        apply_label_style(label(hex), &style).into_any_flex(),
        icon.into_any_flex(),
    ])
    .gap(Length::px(6.0));
    Arc::new(apply_direct_widget_style(
        ecs_button_with_child(ctx.entity, OverlayUiAction::ToggleColorPicker, content),
        &style,
    ))
}

pub(crate) fn project_color_picker_panel(
    panel: &UiColorPickerPanel,
    ctx: ProjectionCtx<'_>,
) -> UiView {
    let pos = match overlay_position(ctx.world, ctx.entity) {
        Some(p) => p,
        None => return hidden_placeholder(),
    };

    let panel_style = default_panel_style(ctx.world, "overlay.color_picker.panel");
    let mut swatch_style = resolve_style_for_classes(ctx.world, ["overlay.color_picker.swatch"]);
    if swatch_style.layout.corner_radius <= 0.0 {
        swatch_style.layout.corner_radius = 3.0;
    }

    let (cur_r, cur_g, cur_b) = ctx
        .world
        .get::<UiColorPicker>(panel.anchor)
        .map(|p| (p.r, p.g, p.b))
        .unwrap_or((255, 255, 255));

    // Build swatch rows (4 rows × 5 cols)
    let mut rows = Vec::new();
    for row in 0..4 {
        let mut row_items = Vec::new();
        for col in 0..5 {
            let idx = row * 5 + col;
            if let Some(&(r, g, b)) = COLOR_SWATCHES.get(idx) {
                let is_selected = r == cur_r && g == cur_g && b == cur_b;
                let mut sw_style = swatch_style.clone();
                sw_style.colors.bg = Some(Color::from_rgb8(r, g, b));
                if is_selected {
                    sw_style.layout.border_width = 2.0;
                    sw_style.colors.border = Some(Color::WHITE);
                }
                let swatch_view = sized_box(label(""))
                    .width(Dim::Fixed(Length::px(28.0)))
                    .height(Dim::Fixed(Length::px(28.0)));
                let swatch_styled = apply_widget_style(swatch_view, &sw_style);
                let btn = ecs_button_with_child(
                    ctx.entity,
                    OverlayUiAction::SelectColorSwatch { r, g, b },
                    swatch_styled,
                );
                row_items.push(apply_direct_widget_style(btn, &swatch_style).into_any_flex());
            }
        }
        rows.push(flex_row(row_items).gap(Length::px(4.0)).into_any_flex());
    }

    let current_hex = format!("#{:02X}{:02X}{:02X}", cur_r, cur_g, cur_b);
    let hex_label = apply_label_style(
        label(current_hex),
        &resolve_style_for_classes(ctx.world, ["overlay.color_picker.value"]),
    );

    let mut panel_items = vec![hex_label.into_any_flex()];
    panel_items.extend(rows);

    let content = flex_col(panel_items).gap(Length::px(6.0));

    let computed_pos = ctx
        .world
        .get::<OverlayComputedPosition>(ctx.entity)
        .copied()
        .unwrap_or_default();
    let panel_width = if computed_pos.width > 1.0 {
        computed_pos.width
    } else {
        260.0
    };
    let panel_height = if computed_pos.height > 1.0 {
        computed_pos.height
    } else {
        200.0
    };

    let panel_view = apply_widget_style(
        crate::xilem::view::portal(content)
            .dims((Length::px(panel_width), Length::px(panel_height))),
        &panel_style,
    );

    Arc::new(
        transformed(crate::views::opaque_hitbox_for_entity(
            ctx.entity, panel_view,
        ))
        .translate(pos),
    )
}

// ---------------------------------------------------------------------------
// Group Box
// ---------------------------------------------------------------------------

pub(crate) fn project_group_box(group_box: &UiGroupBox, ctx: ProjectionCtx<'_>) -> UiView {
    let mut style = resolve_style(ctx.world, ctx.entity);
    if style.layout.border_width <= 0.0 {
        style.layout.border_width = 1.0;
    }
    if style.colors.border.is_none() {
        style.colors.border = Some(Color::from_rgb8(0x3F, 0x3F, 0x3F));
    }
    if style.layout.padding <= 0.0 {
        style.layout.padding = 10.0;
    }

    let title_style = resolve_style_for_classes(ctx.world, ["widget.group_box.title"]);
    let title_view = apply_label_style(label(group_box.title.clone()), &title_style);

    let mut content_items = vec![title_view.into_any_flex()];
    content_items.extend(ctx.children.into_iter().map(|c| c.into_any_flex()));

    Arc::new(apply_widget_style(
        apply_flex_alignment(flex_col(content_items), &style)
            .gap(Length::px(style.layout.gap.max(6.0))),
        &style,
    ))
}

// ---------------------------------------------------------------------------
// Split Pane
// ---------------------------------------------------------------------------

pub(crate) fn project_split_pane(pane: &UiSplitPane, ctx: ProjectionCtx<'_>) -> UiView {
    let fallback: UiView = Arc::new(label(""));
    let child1 = ctx
        .children
        .first()
        .cloned()
        .unwrap_or_else(|| fallback.clone());
    let child2 = ctx
        .children
        .get(1)
        .cloned()
        .unwrap_or_else(|| fallback.clone());

    let axis = match pane.direction {
        SplitDirection::Horizontal => Axis::Horizontal,
        SplitDirection::Vertical => Axis::Vertical,
    };

    Arc::new(
        split(child1, child2)
            .split_axis(axis)
            .split_point(pane.ratio as f64)
            .draggable(true)
            .solid_bar(false),
    )
}

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

pub(crate) fn project_toast(toast: &UiToast, ctx: ProjectionCtx<'_>) -> UiView {
    let mut style = resolve_style(ctx.world, ctx.entity);
    let mut dismiss_style = style.clone();

    // Apply kind-based background color if not overridden
    if style.colors.bg.is_none() {
        style.colors.bg = Some(match toast.kind {
            ToastKind::Info => Color::from_rgb8(0x1D, 0x4E, 0xD8),
            ToastKind::Success => Color::from_rgb8(0x15, 0x80, 0x3D),
            ToastKind::Warning => Color::from_rgb8(0xB4, 0x5D, 0x09),
            ToastKind::Error => Color::from_rgb8(0xB9, 0x1C, 0x1C),
        });
    }
    if style.colors.text.is_none() {
        style.colors.text = Some(Color::WHITE);
    }
    if style.layout.padding <= 0.0 {
        style.layout.padding = 10.0;
    }
    if style.layout.corner_radius <= 0.0 {
        style.layout.corner_radius = 6.0;
    }

    let computed_pos = ctx
        .world
        .get::<OverlayComputedPosition>(ctx.entity)
        .copied()
        .unwrap_or_default();
    let pos = (computed_pos.x, computed_pos.y);

    if !computed_pos.is_positioned {
        hide_style_without_collapsing_layout(&mut style);
        hide_style_without_collapsing_layout(&mut dismiss_style);
    }

    let msg = apply_label_style(label(toast.message.clone()), &style);
    let dismiss = apply_direct_widget_style(
        ecs_button(ctx.entity, OverlayUiAction::DismissToast, "✕".to_string()),
        &dismiss_style,
    );

    let panel = apply_widget_style(
        apply_flex_alignment(
            flex_row(vec![msg.flex(1.0).into_any_flex(), dismiss.into_any_flex()]),
            &style,
        )
        .gap(Length::px(8.0)),
        &style,
    );

    Arc::new(transformed(opaque_hitbox_for_entity(ctx.entity, panel)).translate(pos))
}

// ---------------------------------------------------------------------------
// Date Picker
// ---------------------------------------------------------------------------

pub(crate) fn project_date_picker(picker: &UiDatePicker, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let date_str = format!("{:04}-{:02}-{:02}", picker.year, picker.month, picker.day);
    let icon_color = style
        .colors
        .text
        .unwrap_or(Color::from_rgb8(0xE7, 0xEC, 0xF8));
    let icon = if picker.is_open {
        vector_icon(VectorIcon::ChevronUp, 10.0, icon_color)
    } else {
        vector_icon(VectorIcon::ChevronDown, 10.0, icon_color)
    };
    let content = flex_row(vec![
        apply_label_style(label(date_str), &style).into_any_flex(),
        icon.into_any_flex(),
    ])
    .gap(Length::px(6.0));
    Arc::new(apply_direct_widget_style(
        ecs_button_with_child(ctx.entity, OverlayUiAction::ToggleDatePicker, content),
        &style,
    ))
}

pub(crate) fn project_date_picker_panel(
    panel_comp: &UiDatePickerPanel,
    ctx: ProjectionCtx<'_>,
) -> UiView {
    let pos = match overlay_position(ctx.world, ctx.entity) {
        Some(p) => p,
        None => return hidden_placeholder(),
    };

    let panel_style = default_panel_style(ctx.world, "overlay.date_picker.panel");
    let mut cell_style = resolve_style_for_classes(ctx.world, ["overlay.date_picker.cell"]);
    if cell_style.layout.padding <= 0.0 {
        cell_style.layout.padding = 4.0;
    }
    let mut today_style = cell_style.clone();
    if today_style.colors.border.is_none() {
        today_style.colors.border = Some(Color::from_rgb8(0x00, 0x78, 0xD4));
        today_style.layout.border_width = 1.0;
    }
    let mut selected_style = cell_style.clone();
    selected_style.colors.bg = Some(Color::from_rgb8(0x00, 0x78, 0xD4));

    let view_year = panel_comp.view_year;
    let view_month = panel_comp.view_month;
    let selected_day = ctx
        .world
        .get::<UiDatePicker>(panel_comp.anchor)
        .and_then(|dp| {
            if dp.year == view_year && dp.month == view_month {
                Some(dp.day)
            } else {
                None
            }
        });

    // Navigation row
    let nav_style = resolve_style_for_classes(ctx.world, ["overlay.date_picker.nav"]);
    let prev_btn = ecs_button(
        ctx.entity,
        OverlayUiAction::NavigateDateMonth { forward: false },
        "<".to_string(),
    );
    let next_btn = ecs_button(
        ctx.entity,
        OverlayUiAction::NavigateDateMonth { forward: true },
        ">".to_string(),
    );
    let month_lbl = apply_label_style(
        label(format!("{} {view_year}", month_name(view_month))),
        &nav_style,
    );
    let nav_row = flex_row(vec![
        apply_direct_widget_style(prev_btn, &cell_style).into_any_flex(),
        month_lbl.flex(1.0).into_any_flex(),
        apply_direct_widget_style(next_btn, &cell_style).into_any_flex(),
    ])
    .gap(Length::px(4.0));

    // Day-of-week headers
    let dow_labels = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"].map(|d| {
        apply_label_style(label(d), &cell_style)
            .flex(1.0)
            .into_any_flex()
    });
    let dow_row = flex_row(dow_labels.into_iter().collect::<Vec<_>>());

    // Calendar grid
    let first_dow = day_of_week_for_first(view_year, view_month) as usize;
    let num_days = days_in_month(view_year, view_month) as usize;
    let total_cells = first_dow + num_days;
    let num_rows = (total_cells + 6) / 7;

    let mut week_rows = Vec::new();
    for week in 0..num_rows {
        let mut week_cells = Vec::new();
        for dow in 0..7 {
            let cell_index = week * 7 + dow;
            let day_num = if cell_index < first_dow {
                None
            } else {
                let d = cell_index - first_dow + 1;
                if d <= num_days { Some(d as u32) } else { None }
            };

            let cell: UiView = if let Some(day) = day_num {
                let s = if Some(day) == selected_day {
                    &selected_style
                } else {
                    &cell_style
                };
                let btn = ecs_button(
                    ctx.entity,
                    OverlayUiAction::SelectDateDay { day },
                    day.to_string(),
                );
                Arc::new(apply_direct_widget_style(btn, s))
            } else {
                Arc::new(apply_label_style(label(""), &cell_style))
            };
            week_cells.push(cell.flex(1.0).into_any_flex());
        }
        week_rows.push(flex_row(week_cells).gap(Length::px(2.0)).into_any_flex());
    }

    let mut all_rows = vec![nav_row.into_any_flex(), dow_row.into_any_flex()];
    all_rows.extend(week_rows);

    let content = flex_col(all_rows).gap(Length::px(4.0));

    let computed_pos = ctx
        .world
        .get::<OverlayComputedPosition>(ctx.entity)
        .copied()
        .unwrap_or_default();
    let panel_width = if computed_pos.width > 1.0 {
        computed_pos.width
    } else {
        280.0
    };
    let panel_height = if computed_pos.height > 1.0 {
        computed_pos.height
    } else {
        300.0
    };

    let panel_view = apply_widget_style(
        crate::xilem::view::portal(content)
            .dims((Length::px(panel_width), Length::px(panel_height))),
        &panel_style,
    );

    Arc::new(
        transformed(crate::views::opaque_hitbox_for_entity(
            ctx.entity, panel_view,
        ))
        .translate(pos),
    )
}
