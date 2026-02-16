use bevy_ecs::entity::Entity;
use masonry::core::ArcStr;
use xilem_core::{Arg, MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx};

use crate::widgets::EcsButtonWidget;

/// View for [`EcsButtonWidget`].
#[must_use = "View values do nothing unless returned into the synthesized UI tree."]
pub struct EcsButtonView<A> {
    entity: Entity,
    action: A,
    label: ArcStr,
}

#[must_use]
pub fn ecs_button<A>(entity: Entity, action: A, label: impl Into<ArcStr>) -> EcsButtonView<A>
where
    A: Clone + Send + Sync + 'static,
{
    EcsButtonView {
        entity,
        action,
        label: label.into(),
    }
}

impl<A> ViewMarker for EcsButtonView<A> where A: Clone + Send + Sync + 'static {}

impl<A> View<(), (), ViewCtx> for EcsButtonView<A>
where
    A: Clone + Send + Sync + 'static,
{
    type Element = Pod<EcsButtonWidget<A>>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: Arg<'_, ()>,
    ) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(EcsButtonWidget::new(
                self.entity,
                self.action.clone(),
                self.label.clone(),
            )),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: Arg<'_, ()>,
    ) {
        if self.entity != prev.entity {
            EcsButtonWidget::set_entity(&mut element, self.entity);
        }
        if self.label != prev.label {
            EcsButtonWidget::set_label(&mut element, self.label.clone());
        }

        // Action update is applied unconditionally to avoid requiring extra trait bounds.
        EcsButtonWidget::set_action(&mut element, self.action.clone());
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        _message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: Arg<'_, ()>,
    ) -> MessageResult<()> {
        MessageResult::Nop
    }
}
