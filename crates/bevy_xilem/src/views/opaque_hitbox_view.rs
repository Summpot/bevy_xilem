use std::marker::PhantomData;

use bevy_ecs::entity::Entity;
use xilem_core::{Arg, MessageCtx, MessageResult, Mut, View, ViewArgument, ViewMarker};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

use crate::widgets::OpaqueHitboxWidget;

#[must_use]
pub fn opaque_hitbox<Child, State, Action>(child: Child) -> OpaqueHitboxView<Child, State, Action>
where
    Child: WidgetView<State, Action>,
    State: ViewArgument,
{
    OpaqueHitboxView {
        entity: None,
        child,
        phantom: PhantomData,
    }
}

#[must_use]
pub fn opaque_hitbox_for_entity<Child, State, Action>(
    entity: Entity,
    child: Child,
) -> OpaqueHitboxView<Child, State, Action>
where
    Child: WidgetView<State, Action>,
    State: ViewArgument,
{
    OpaqueHitboxView {
        entity: Some(entity),
        child,
        phantom: PhantomData,
    }
}

/// Wraps a child widget in a pointer-opaque hitbox.
pub struct OpaqueHitboxView<Child, State, Action> {
    entity: Option<Entity>,
    child: Child,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<Child, State, Action> ViewMarker for OpaqueHitboxView<Child, State, Action> {}

impl<Child, State, Action> View<State, Action, ViewCtx> for OpaqueHitboxView<Child, State, Action>
where
    Child: WidgetView<State, Action>,
    State: ViewArgument,
    Action: 'static,
{
    type Element = Pod<OpaqueHitboxWidget>;
    type ViewState = Child::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        app_state: Arg<'_, State>,
    ) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.child.build(ctx, app_state);
        let widget = match self.entity {
            Some(entity) => OpaqueHitboxWidget::new_for_entity(entity, child.new_widget),
            None => OpaqueHitboxWidget::new(child.new_widget),
        };

        (ctx.create_pod(widget), child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: Arg<'_, State>,
    ) {
        if self.entity != prev.entity {
            OpaqueHitboxWidget::set_entity(&mut element, self.entity);
        }

        let mut child = OpaqueHitboxWidget::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = OpaqueHitboxWidget::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: Arg<'_, State>,
    ) -> MessageResult<Action> {
        let mut child = OpaqueHitboxWidget::child_mut(&mut element);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}
