use bevy_ecs::{entity::Entity, prelude::*};

use crate::{ProjectionCtx, UiView, components::UiComponentTemplate};

/// Tab bar component that shows labeled tabs and manages active content.
///
/// Place tab content entities as ECS children; the active tab index
/// determines which child is displayed.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiTabBar {
    /// Labels shown on each tab header.
    pub tabs: Vec<String>,
    /// Index of the currently active tab.
    pub active: usize,
}

impl UiTabBar {
    #[must_use]
    pub fn new(tabs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            tabs: tabs.into_iter().map(Into::into).collect(),
            active: 0,
        }
    }

    #[must_use]
    pub fn with_active(mut self, index: usize) -> Self {
        self.active = index;
        self
    }
}

/// Emitted when the active tab changes in a [`UiTabBar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiTabChanged {
    pub bar: Entity,
    pub active: usize,
}

impl UiComponentTemplate for UiTabBar {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_tab_bar(component, ctx)
    }
}
