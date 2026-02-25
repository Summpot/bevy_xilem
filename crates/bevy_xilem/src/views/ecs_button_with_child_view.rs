use bevy_ecs::entity::Entity;
use xilem_core::{Arg, MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

use crate::widgets::{EcsButtonWidgetAction, EcsButtonWithChildWidget};

/// ECS-dispatched button view that accepts an arbitrary child widget view.
#[must_use = "View values do nothing unless returned into the synthesized UI tree."]
pub struct EcsButtonWithChildView<A, Child> {
    entity: Entity,
    action: A,
    child: Child,
}

#[must_use]
pub fn ecs_button_with_child<A, Child>(
    entity: Entity,
    action: A,
    child: Child,
) -> EcsButtonWithChildView<A, Child>
where
    A: Clone + Send + Sync + 'static,
    Child: WidgetView<(), ()>,
{
    EcsButtonWithChildView {
        entity,
        action,
        child,
    }
}

impl<A, Child> ViewMarker for EcsButtonWithChildView<A, Child>
where
    A: Clone + Send + Sync + 'static,
    Child: WidgetView<(), ()>,
{
}

impl<A, Child> View<(), (), ViewCtx> for EcsButtonWithChildView<A, Child>
where
    A: Clone + Send + Sync + 'static,
    Child: WidgetView<(), ()>,
{
    type Element = Pod<EcsButtonWithChildWidget<A>>;
    type ViewState = Child::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: Arg<'_, ()>) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.child.build(ctx, app_state);

        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(EcsButtonWithChildWidget::new(
                    self.entity,
                    self.action.clone(),
                    child.new_widget,
                ))
            }),
            child_state,
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: Arg<'_, ()>,
    ) {
        if self.entity != prev.entity {
            EcsButtonWithChildWidget::set_entity(&mut element, self.entity);
        }

        EcsButtonWithChildWidget::set_action(&mut element, self.action.clone());

        let mut child = EcsButtonWithChildWidget::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        {
            let mut child = EcsButtonWithChildWidget::child_mut(&mut element);
            self.child.teardown(view_state, ctx, child.downcast());
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: Arg<'_, ()>,
    ) -> MessageResult<()> {
        if !message.remaining_path().is_empty() {
            let mut child = EcsButtonWithChildWidget::child_mut(&mut element);
            return self
                .child
                .message(view_state, message, child.downcast(), app_state);
        }

        match message.take_message::<EcsButtonWidgetAction>() {
            Some(_) => MessageResult::Action(()),
            None => MessageResult::Stale,
        }
    }
}
