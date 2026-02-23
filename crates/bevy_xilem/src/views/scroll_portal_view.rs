use std::marker::PhantomData;

use masonry::{kurbo::Point, widgets};
use xilem_core::{Arg, MessageCtx, MessageResult, Mut, View, ViewArgument, ViewMarker};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

/// Portal view with explicit viewport-position configuration.
#[must_use]
pub fn scroll_portal<Child, State, Action>(
    child: Child,
    viewport_pos: Point,
) -> ScrollPortalView<Child, State, Action>
where
    State: ViewArgument,
    Child: WidgetView<State, Action>,
{
    ScrollPortalView {
        child,
        viewport_pos,
        constrain_horizontal: false,
        constrain_vertical: false,
        content_must_fill: false,
        phantom: PhantomData,
    }
}

/// The [`View`] created by [`scroll_portal`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ScrollPortalView<V, State, Action> {
    child: V,
    viewport_pos: Point,
    constrain_horizontal: bool,
    constrain_vertical: bool,
    content_must_fill: bool,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ScrollPortalView<V, State, Action> {
    #[must_use]
    pub fn constrain_horizontal(mut self, constrain: bool) -> Self {
        self.constrain_horizontal = constrain;
        self
    }

    #[must_use]
    pub fn constrain_vertical(mut self, constrain: bool) -> Self {
        self.constrain_vertical = constrain;
        self
    }

    #[must_use]
    pub fn content_must_fill(mut self, must_fill: bool) -> Self {
        self.content_must_fill = must_fill;
        self
    }
}

impl<V, State, Action> ViewMarker for ScrollPortalView<V, State, Action> {}

impl<Child, State, Action> View<State, Action, ViewCtx> for ScrollPortalView<Child, State, Action>
where
    Child: WidgetView<State, Action>,
    State: ViewArgument,
    Action: 'static,
{
    type Element = Pod<widgets::Portal<Child::Widget>>;
    type ViewState = Child::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        app_state: Arg<'_, State>,
    ) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.child.build(ctx, app_state);
        let widget_pod = ctx.create_pod(
            widgets::Portal::new(child.new_widget)
                .constrain_horizontal(self.constrain_horizontal)
                .constrain_vertical(self.constrain_vertical)
                .content_must_fill(self.content_must_fill),
        );
        (widget_pod, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: Arg<'_, State>,
    ) {
        let child_element = widgets::Portal::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child_element, app_state);

        if self.constrain_horizontal != prev.constrain_horizontal {
            widgets::Portal::set_constrain_horizontal(&mut element, self.constrain_horizontal);
        }
        if self.constrain_vertical != prev.constrain_vertical {
            widgets::Portal::set_constrain_vertical(&mut element, self.constrain_vertical);
        }
        if self.content_must_fill != prev.content_must_fill {
            widgets::Portal::set_content_must_fill(&mut element, self.content_must_fill);
        }

        if self.viewport_pos != prev.viewport_pos {
            widgets::Portal::set_viewport_pos(&mut element, self.viewport_pos);
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let child_element = widgets::Portal::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child_element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: Arg<'_, State>,
    ) -> MessageResult<Action> {
        let child_element = widgets::Portal::child_mut(&mut element);
        self.child
            .message(view_state, message, child_element, app_state)
    }
}
