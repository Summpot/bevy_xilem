use bevy_ecs::prelude::*;

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

/// The split axis for a [`UiSplitPane`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SplitDirection {
    /// Children are placed side by side (left / right).
    #[default]
    Horizontal,
    /// Children are stacked (top / bottom).
    Vertical,
}

/// A two-panel split container with a draggable divider.
///
/// Place exactly two ECS child entities; they become the first and second
/// panels. The divider is draggable by default.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct UiSplitPane {
    /// Fractional size of the first panel (0.0 – 1.0).
    pub ratio: f32,
    pub direction: SplitDirection,
}

impl UiSplitPane {
    #[must_use]
    pub fn new(ratio: f32) -> Self {
        Self {
            ratio: ratio.clamp(0.05, 0.95),
            direction: SplitDirection::Horizontal,
        }
    }

    #[must_use]
    pub fn vertical(mut self) -> Self {
        self.direction = SplitDirection::Vertical;
        self
    }
}

impl UiControlTemplate for UiSplitPane {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_split_pane(component, ctx)
    }
}
