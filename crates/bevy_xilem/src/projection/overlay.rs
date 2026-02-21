use super::core::{ProjectionCtx, UiView};
use crate::{
    ecs::{OverlayStack, OverlayState, UiOverlayRoot},
    styling::{apply_widget_style, resolve_style_for_classes},
};
use masonry::layout::{Dim, UnitPoint};
use std::sync::Arc;
use xilem_masonry::style::Style;
use xilem_masonry::view::{label, zstack};

pub(crate) fn project_overlay_root(_: &UiOverlayRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let has_modal_overlay = ctx
        .world
        .get_resource::<OverlayStack>()
        .is_some_and(|stack| {
            stack.active_overlays.iter().any(|overlay| {
                ctx.world
                    .get::<OverlayState>(*overlay)
                    .is_some_and(|state| state.is_modal)
            })
        });

    let mut layers = Vec::with_capacity(ctx.children.len() + usize::from(has_modal_overlay));

    if has_modal_overlay {
        let mut dimmer_style = resolve_style_for_classes(ctx.world, ["overlay.modal.dimmer"]);
        if dimmer_style.colors.bg.is_none() {
            dimmer_style.colors.bg = Some(xilem::Color::from_rgba8(0, 0, 0, 160));
        }

        let dimmer: UiView = Arc::new(apply_widget_style(
            xilem_masonry::view::sized_box(label(""))
                .width(Dim::Stretch)
                .height(Dim::Stretch),
            &dimmer_style,
        ));
        layers.push(dimmer);
    }

    layers.extend(ctx.children);

    Arc::new(
        zstack(layers)
            .alignment(UnitPoint::TOP_LEFT)
            .width(Dim::Stretch)
            .height(Dim::Stretch),
    )
}
