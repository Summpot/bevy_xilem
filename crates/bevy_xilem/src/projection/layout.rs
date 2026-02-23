use super::core::{ProjectionCtx, UiView};
use crate::{
    ecs::{UiFlexColumn, UiFlexRow, UiRoot},
    styling::{apply_flex_alignment, apply_widget_style, resolve_style},
};
use masonry::layout::{Dim, Length};
use std::sync::Arc;
use xilem_masonry::style::Style;
use xilem_masonry::view::{FlexExt as _, flex_col, flex_row};

pub(crate) fn project_ui_root(_: &UiRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        apply_flex_alignment(flex_col(children), &style)
            .gap(Length::px(style.layout.gap))
            .width(Dim::Stretch)
            .height(Dim::Stretch),
        &style,
    ))
}

pub(crate) fn project_flex_column(_: &UiFlexColumn, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        apply_flex_alignment(flex_col(children), &style).gap(Length::px(style.layout.gap)),
        &style,
    ))
}

pub(crate) fn project_flex_row(_: &UiFlexRow, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        apply_flex_alignment(flex_row(children), &style).gap(Length::px(style.layout.gap)),
        &style,
    ))
}
