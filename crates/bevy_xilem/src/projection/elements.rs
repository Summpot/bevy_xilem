use super::{
    core::{BuiltinUiAction, ProjectionCtx, UiView},
    utils::localized_font_stack,
};
use crate::{
    ecs::{LocalizeText, UiButton, UiLabel},
    i18n::resolve_localized_text,
    styling::{apply_direct_widget_style, apply_label_style, resolve_style},
    views::ecs_button_with_child,
};
use std::sync::Arc;
use tracing::trace;
use xilem_masonry::view::label;

pub(crate) fn project_label(label_component: &UiLabel, ctx: ProjectionCtx<'_>) -> UiView {
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

pub(crate) fn project_button(button_component: &UiButton, ctx: ProjectionCtx<'_>) -> UiView {
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
