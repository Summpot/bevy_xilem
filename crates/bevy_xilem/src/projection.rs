use std::{fmt, marker::PhantomData, sync::Arc};

use bevy_ecs::prelude::*;
use masonry::layout::{Dim, Length, UnitPoint};
use tracing::trace;
use xilem::style::Style as _;
use xilem_masonry::{
    AnyWidgetView,
    view::{FlexExt as _, ZStackExt as _, flex_col, flex_row, label, transformed, zstack},
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

    let backdrop = xilem_masonry::view::sized_box(apply_direct_widget_style(
        ecs_button(ctx.entity, OverlayUiAction::DismissDialog, ""),
        &backdrop_style,
    ))
    .width(Dim::Stretch)
    .height(Dim::Stretch)
    .alignment(UnitPoint::TOP_LEFT);

    let dialog_panel = apply_widget_style(
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
    );

    Arc::new(
        zstack((backdrop, dialog_panel))
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

    let items = anchor
        .and_then(|anchor| ctx.world.get::<UiComboBox>(anchor))
        .map(|combo_box| {
            combo_box
                .options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    let label_text =
                        translate_text(ctx.world, option.label_key.as_deref(), &option.label);
                    apply_direct_widget_style(
                        ecs_button(
                            ctx.entity,
                            OverlayUiAction::SelectComboItem { index },
                            label_text,
                        ),
                        &item_style,
                    )
                    .into_any_flex()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let anchor_rect = ctx
        .world
        .get::<OverlayAnchorRect>(ctx.entity)
        .copied()
        .unwrap_or_default();

    let dropdown_panel = transformed(apply_widget_style(
        flex_col(items).gap(Length::px(menu_style.layout.gap.max(6.0))),
        &menu_style,
    ))
    .translate((anchor_rect.left, anchor_rect.top + anchor_rect.height + 4.0));

    let dropdown_panel = xilem_masonry::view::sized_box(dropdown_panel)
        .fixed_width(Length::px(anchor_rect.width.max(1.0)));

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
