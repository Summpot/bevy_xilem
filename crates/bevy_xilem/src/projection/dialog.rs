use super::{
    core::{ProjectionCtx, UiView},
    utils::{
        app_i18n_font_stack, estimate_text_width_px, estimate_wrapped_lines,
        hide_style_without_collapsing_layout, translate_text,
    },
};
use crate::{
    ecs::{OverlayComputedPosition, UiDialog},
    overlay::OverlayUiAction,
    styling::{
        apply_direct_widget_style, apply_label_style, apply_widget_style, resolve_style,
        resolve_style_for_classes,
    },
    views::{ecs_button, opaque_hitbox_for_entity},
};
use masonry::layout::Length;
use std::sync::Arc;
use xilem::{palette::css::BLACK, style::BoxShadow, style::Style as _};
use xilem_masonry::view::{CrossAxisAlignment, FlexExt as _, flex_col, label, transformed};

pub(crate) const DIALOG_SURFACE_MIN_WIDTH: f64 = 240.0;
pub(crate) const DIALOG_SURFACE_MAX_WIDTH: f64 = 400.0;

pub(crate) fn estimate_dialog_surface_width_px(
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

pub(crate) fn estimate_dialog_surface_height_px(
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

pub(crate) fn project_dialog(dialog: &UiDialog, ctx: ProjectionCtx<'_>) -> UiView {
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

    if !computed_position.is_positioned {
        hide_style_without_collapsing_layout(&mut dialog_style);
        hide_style_without_collapsing_layout(&mut title_style);
        hide_style_without_collapsing_layout(&mut body_style);
        hide_style_without_collapsing_layout(&mut dismiss_style);
    }

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

    let dismiss_button = apply_direct_widget_style(
        ecs_button(
            ctx.entity,
            OverlayUiAction::DismissDialog,
            dismiss_label.clone(),
        ),
        &dismiss_style,
    )
    .into_any_flex();

    let mut dialog_children = vec![
        apply_label_style(label(title), &title_style).into_any_flex(),
        apply_label_style(label(body), &body_style).into_any_flex(),
    ];
    dialog_children.extend(ctx.children.into_iter().map(|child| child.into_any_flex()));
    dialog_children.push(dismiss_button);

    let dialog_surface = xilem_masonry::view::sized_box(apply_widget_style(
        flex_col(dialog_children)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(Length::px(dialog_gap)),
        &dialog_style,
    ))
    .fixed_width(Length::px(dialog_surface_width))
    .fixed_height(Length::px(dialog_surface_height));

    let dialog_panel = transformed(opaque_hitbox_for_entity(ctx.entity, dialog_surface))
        .translate((computed_position.x, computed_position.y));

    Arc::new(dialog_panel)
}

#[cfg(test)]
mod tests {
    use super::{
        DIALOG_SURFACE_MAX_WIDTH, DIALOG_SURFACE_MIN_WIDTH, estimate_dialog_surface_width_px,
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
}
