use super::{
    core::{BuiltinUiAction, ProjectionCtx, UiView},
    utils::localized_font_stack,
};
use crate::{
    ecs::{
        LocalizeText, PartCheckboxIndicator, PartCheckboxLabel, PartComboBoxChevron,
        PartComboBoxDisplay, PartDialogBody, PartDialogDismiss, PartDialogTitle,
        PartSliderDecrease, PartSliderIncrease, PartSliderThumb, PartSliderTrack, PartSwitchThumb,
        PartSwitchTrack, UiButton, UiCheckbox, UiComboBox, UiDialog, UiLabel, UiSlider, UiSwitch,
        UiTextInput,
    },
    i18n::resolve_localized_text,
    overlay::OverlayUiAction,
    styling::{
        apply_direct_widget_style, apply_label_style, apply_text_input_style, apply_widget_style,
        resolve_style,
    },
    views::{ecs_button_with_child, ecs_text_input},
    widget_actions::WidgetUiAction,
};
use bevy_ecs::{hierarchy::Children, prelude::*};
use masonry::layout::Length;
use std::sync::Arc;
use tracing::trace;
use xilem_masonry::style::Style as _;
use xilem_masonry::view::{FlexExt as _, flex_row, label};

fn child_entity_views(ctx: &ProjectionCtx<'_>) -> Vec<(Entity, UiView)> {
    let child_entities = ctx
        .world
        .get::<Children>(ctx.entity)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();

    child_entities
        .into_iter()
        .zip(ctx.children.iter().cloned())
        .collect::<Vec<_>>()
}

fn first_part_view<P: Component>(
    ctx: &ProjectionCtx<'_>,
    pairs: &[(Entity, UiView)],
) -> Option<UiView> {
    pairs
        .iter()
        .find_map(|(entity, view)| ctx.world.get::<P>(*entity).map(|_| view.clone()))
}

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

pub(crate) fn project_checkbox(checkbox: &UiCheckbox, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let parts = child_entity_views(&ctx);

    let indicator = first_part_view::<PartCheckboxIndicator>(&ctx, &parts)
        .unwrap_or_else(|| Arc::new(label(if checkbox.checked { "☑" } else { "☐" })));
    let label_view = first_part_view::<PartCheckboxLabel>(&ctx, &parts)
        .unwrap_or_else(|| Arc::new(label(checkbox.label.clone())));

    let content = flex_row(vec![indicator.into_any_flex(), label_view.into_any_flex()])
        .gap(Length::px(style.layout.gap.max(6.0)));

    Arc::new(apply_direct_widget_style(
        ecs_button_with_child(
            ctx.entity,
            WidgetUiAction::ToggleCheckbox {
                checkbox: ctx.entity,
            },
            content,
        ),
        &style,
    ))
}

pub(crate) fn project_slider(slider: &UiSlider, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let parts = child_entity_views(&ctx);

    let dec =
        first_part_view::<PartSliderDecrease>(&ctx, &parts).unwrap_or_else(|| Arc::new(label("−")));
    let track = first_part_view::<PartSliderTrack>(&ctx, &parts)
        .unwrap_or_else(|| Arc::new(label(format!("{:.2}", slider.value))));
    let thumb =
        first_part_view::<PartSliderThumb>(&ctx, &parts).unwrap_or_else(|| Arc::new(label("●")));
    let inc =
        first_part_view::<PartSliderIncrease>(&ctx, &parts).unwrap_or_else(|| Arc::new(label("+")));

    let content = flex_row(vec![
        ecs_button_with_child(
            ctx.entity,
            WidgetUiAction::StepSlider {
                slider: ctx.entity,
                delta: -1.0,
            },
            dec,
        )
        .into_any_flex(),
        track.into_any_flex(),
        thumb.into_any_flex(),
        ecs_button_with_child(
            ctx.entity,
            WidgetUiAction::StepSlider {
                slider: ctx.entity,
                delta: 1.0,
            },
            inc,
        )
        .into_any_flex(),
    ])
    .gap(Length::px(style.layout.gap.max(8.0)));

    Arc::new(apply_widget_style(content, &style))
}

pub(crate) fn project_switch(switch_control: &UiSwitch, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let parts = child_entity_views(&ctx);

    let track = first_part_view::<PartSwitchTrack>(&ctx, &parts)
        .unwrap_or_else(|| Arc::new(label(if switch_control.on { "On" } else { "Off" })));
    let thumb =
        first_part_view::<PartSwitchThumb>(&ctx, &parts).unwrap_or_else(|| Arc::new(label("●")));

    let content = flex_row(vec![track.into_any_flex(), thumb.into_any_flex()])
        .gap(Length::px(style.layout.gap.max(8.0)));

    Arc::new(apply_direct_widget_style(
        ecs_button_with_child(
            ctx.entity,
            WidgetUiAction::ToggleSwitch { switch: ctx.entity },
            content,
        ),
        &style,
    ))
}

pub(crate) fn project_text_input(input: &UiTextInput, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    Arc::new(apply_widget_style(
        apply_text_input_style(
            ecs_text_input(ctx.entity, input.value.clone(), move |value| {
                WidgetUiAction::SetTextInput {
                    input: ctx.entity,
                    value,
                }
            }),
            &style,
        ),
        &style,
    ))
}

pub(crate) fn project_dialog(dialog: &UiDialog, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let parts = child_entity_views(&ctx);

    let title = first_part_view::<PartDialogTitle>(&ctx, &parts)
        .unwrap_or_else(|| Arc::new(label(dialog.title.clone())));
    let body = first_part_view::<PartDialogBody>(&ctx, &parts)
        .unwrap_or_else(|| Arc::new(label(dialog.body.clone())));
    let dismiss_label = first_part_view::<PartDialogDismiss>(&ctx, &parts)
        .unwrap_or_else(|| Arc::new(label(dialog.dismiss_label.clone())));

    let dismiss = ecs_button_with_child(ctx.entity, OverlayUiAction::DismissDialog, dismiss_label);

    let mut content = vec![title.into_any_flex(), body.into_any_flex()];
    content.extend(parts.into_iter().filter_map(|(entity, view)| {
        (ctx.world.get::<PartDialogTitle>(entity).is_none()
            && ctx.world.get::<PartDialogBody>(entity).is_none()
            && ctx.world.get::<PartDialogDismiss>(entity).is_none())
        .then_some(view.into_any_flex())
    }));
    content.push(dismiss.into_any_flex());

    Arc::new(apply_widget_style(
        xilem_masonry::view::flex_col(content).gap(Length::px(style.layout.gap.max(10.0))),
        &style,
    ))
}

pub(crate) fn project_combo_box(combo_box: &UiComboBox, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let parts = child_entity_views(&ctx);

    let display = first_part_view::<PartComboBoxDisplay>(&ctx, &parts).unwrap_or_else(|| {
        let selected = combo_box
            .clamped_selected()
            .and_then(|index| combo_box.options.get(index))
            .map(|opt| opt.label.clone())
            .unwrap_or_else(|| combo_box.placeholder.clone());
        Arc::new(label(selected))
    });
    let chevron = first_part_view::<PartComboBoxChevron>(&ctx, &parts)
        .unwrap_or_else(|| Arc::new(label(if combo_box.is_open { "▴" } else { "▾" })));

    Arc::new(apply_direct_widget_style(
        ecs_button_with_child(
            ctx.entity,
            OverlayUiAction::ToggleCombo,
            flex_row(vec![display.into_any_flex(), chevron.into_any_flex()])
                .gap(Length::px(style.layout.gap.max(8.0))),
        ),
        &style,
    ))
}
