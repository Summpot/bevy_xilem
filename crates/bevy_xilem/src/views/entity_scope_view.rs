use std::marker::PhantomData;

use bevy_ecs::entity::Entity;
use xilem_core::{Arg, MessageCtx, MessageResult, Mut, View, ViewArgument, ViewMarker};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

use crate::widgets::EntityScopeWidget;

#[must_use]
pub fn entity_scope<Child, State, Action>(
    entity: Entity,
    child: Child,
) -> EntityScopeView<Child, State, Action>
where
    Child: WidgetView<State, Action>,
    State: ViewArgument,
{
    EntityScopeView {
        entity,
        child,
        phantom: PhantomData,
    }
}

/// Wrap a child view with an entity-bound Masonry widget scope.
pub struct EntityScopeView<Child, State, Action> {
    entity: Entity,
    child: Child,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<Child, State, Action> ViewMarker for EntityScopeView<Child, State, Action> {}

impl<Child, State, Action> View<State, Action, ViewCtx> for EntityScopeView<Child, State, Action>
where
    Child: WidgetView<State, Action>,
    State: ViewArgument,
    Action: 'static,
{
    type Element = Pod<EntityScopeWidget>;
    type ViewState = Child::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        app_state: Arg<'_, State>,
    ) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.child.build(ctx, app_state);
        (
            ctx.create_pod(EntityScopeWidget::new(self.entity, child.new_widget)),
            child_state,
        )
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
            EntityScopeWidget::set_entity(&mut element, self.entity);
        }

        let mut child = EntityScopeWidget::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = EntityScopeWidget::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: Arg<'_, State>,
    ) -> MessageResult<Action> {
        let mut child = EntityScopeWidget::child_mut(&mut element);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}
