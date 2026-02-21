use super::core::{ProjectionCtx, UiView};
use crate::{
    ecs::{UiFlexColumn, UiFlexRow},
    styling::{apply_widget_style, resolve_style},
};
use masonry::layout::Length;
use std::sync::Arc;
use xilem_masonry::style::Style;
use xilem_masonry::view::{FlexExt as _, flex_col, flex_row};

pub(crate) fn project_flex_column(_: &UiFlexColumn, ctx: ProjectionCtx<'_>) -> UiView {
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

pub(crate) fn project_flex_row(_: &UiFlexRow, ctx: ProjectionCtx<'_>) -> UiView {
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
