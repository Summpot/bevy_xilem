use std::{fmt, marker::PhantomData, sync::Arc};

use bevy_ecs::prelude::*;
use masonry::layout::{Dim, Length, UnitPoint};
use tracing::trace;
use xilem::{palette::css::BLACK, style::BoxShadow, style::Style as _};
use xilem_masonry::{
    AnyWidgetView,
    view::{
        CrossAxisAlignment, FlexExt as _, MainAxisAlignment, ZStackExt as _, flex_col, flex_row,
        label, portal, transformed, zstack,
    },
};

use crate::{
    ecs::{
        AnchoredTo, LocalizeText, OverlayAnchorRect, UiButton, UiComboBox, UiDialog,
        UiDropdownMenu, UiFlexColumn, UiFlexRow, UiLabel, UiOverlayRoot,
    },
    i18n::{AppI18n, resolve_localized_text},
    overlay::OverlayUiAction,
    styling::{
        apply_direct_widget_style, apply_label_style, apply_widget_style, resolve_style,
        resolve_style_for_classes,
    },
    views::{ecs_button, ecs_button_with_child},
};

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

    let dialog_surface_width = estimate_dialog_surface_width_px(
        &title,
        &body,
        &dismiss_label,
        title_style.text.size,
        body_style.text.size,
        dismiss_style.text.size,
        dialog_style.layout.padding.max(12.0),
    );

    let backdrop = xilem_masonry::view::sized_box(apply_direct_widget_style(
        ecs_button(ctx.entity, OverlayUiAction::DismissDialog, ""),
        &backdrop_style,
    ))
    .width(Dim::Stretch)
    .height(Dim::Stretch)
    .alignment(UnitPoint::TOP_LEFT);

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
        .gap(Length::px(dialog_style.layout.gap.max(10.0))),
        &dialog_style,
    ))
    .fixed_width(Length::px(dialog_surface_width));

    let centered_surface_layer = flex_col((dialog_surface.into_any_flex(),))
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .width(Dim::Stretch)
        .height(Dim::Stretch);

    Arc::new(
        zstack((backdrop, centered_surface_layer))
            .alignment(UnitPoint::CENTER)
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

    let dropdown_width = estimate_dropdown_surface_width_px(
        anchor_rect.width.max(1.0),
        translated_options.iter().map(String::as_str),
        item_style.text.size,
        item_style.layout.padding * 2.0 + menu_style.layout.padding * 2.0,
    );

    let item_gap = menu_style.layout.gap.max(6.0);
    let dropdown_height = estimate_dropdown_viewport_height_px(
        translated_options.len(),
        item_style.text.size,
        item_style.layout.padding,
        item_gap,
    );

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
        .translate((anchor_rect.left, anchor_rect.top + anchor_rect.height + 4.0));

    let backdrop_style = resolve_style_for_classes(ctx.world, ["overlay.dropdown.backdrop"]);
    let backdrop = xilem_masonry::view::sized_box(apply_direct_widget_style(
        ecs_button(ctx.entity, OverlayUiAction::DismissDropdown, ""),
        &backdrop_style,
    ))
    .width(Dim::Stretch)
    .height(Dim::Stretch)
    .alignment(UnitPoint::TOP_LEFT);

    Arc::new(
        zstack((backdrop, dropdown_panel.alignment(UnitPoint::TOP_LEFT)))
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
        estimate_dialog_surface_width_px, estimate_dropdown_surface_width_px,
        estimate_dropdown_viewport_height_px,
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
}
