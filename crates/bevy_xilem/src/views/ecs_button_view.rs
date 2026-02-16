use bevy_ecs::entity::Entity;
use masonry::core::ArcStr;
use xilem_core::{Arg, MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx};

use crate::widgets::{EcsButtonWidget, EcsButtonWidgetAction};

/// ECS-dispatched view backed by Masonry's native `Button` widget.
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
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(EcsButtonWidget::new(
                    self.entity,
                    self.action.clone(),
                    self.label.clone(),
                ))
            }),
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

        EcsButtonWidget::set_action(&mut element, self.action.clone());

        if self.label != prev.label {
            EcsButtonWidget::set_label(&mut element, self.label.clone());
        }
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        _message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: Arg<'_, ()>,
    ) -> MessageResult<()> {
        match _message.take_first() {
            None => match _message.take_message::<EcsButtonWidgetAction>() {
                Some(_) => MessageResult::Action(()),
                None => MessageResult::Stale,
            },
            _ => MessageResult::Stale,
        }
    }
}
