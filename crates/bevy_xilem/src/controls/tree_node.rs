use bevy_ecs::{entity::Entity, prelude::*};

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

/// A node in a tree view hierarchy.
///
/// Tree nodes are connected through ECS parent/child relationships.
/// A node with `UiTreeNode` children shows an expand/collapse toggle.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiTreeNode {
    /// Display label for this node.
    pub label: String,
    /// Whether children are currently visible.
    pub is_expanded: bool,
}

impl UiTreeNode {
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            is_expanded: false,
        }
    }

    #[must_use]
    pub fn expanded(mut self) -> Self {
        self.is_expanded = true;
        self
    }
}

/// Emitted when a tree node is expanded or collapsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiTreeNodeToggled {
    pub node: Entity,
    pub is_expanded: bool,
}

impl UiControlTemplate for UiTreeNode {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_tree_node(component, ctx)
    }
}
