use bevy_ecs::entity::Entity;
use masonry::{
    core::ArcStr,
    widgets::{self, ButtonPress},
};
use xilem_core::{Arg, MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx};

use crate::events::emit_ui_action;

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
    type Element = Pod<widgets::Button>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: Arg<'_, ()>,
    ) -> (Self::Element, Self::ViewState) {
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(widgets::Button::with_text(self.label.clone()))
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
        if self.label != prev.label {
            let mut child_mut = widgets::Button::child_mut(&mut element);
            let mut child = child_mut.downcast::<widgets::Label>();
            widgets::Label::set_text(&mut child, self.label.clone());
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
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: Arg<'_, ()>,
    ) -> MessageResult<()> {
        if message.take_first().is_some() {
            return MessageResult::Stale;
        }

        match message.take_message::<ButtonPress>() {
            Some(press) => {
                if press.button.is_none()
                    || press.button == Some(masonry::core::PointerButton::Primary)
                {
                    emit_ui_action(self.entity, self.action.clone());
                }
                MessageResult::Nop
            }
            None => MessageResult::Stale,
        }
    }
}
