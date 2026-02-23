use bevy_ecs::prelude::*;

use crate::{ProjectionCtx, UiView, components::UiComponentTemplate};

/// A simple data table with column headers and rows.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiTable {
    /// Column header labels.
    pub columns: Vec<String>,
    /// Table data rows (each row is a list of cell strings).
    pub rows: Vec<Vec<String>>,
}

impl UiTable {
    #[must_use]
    pub fn new(columns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            columns: columns.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_row(mut self, cells: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.rows.push(cells.into_iter().map(Into::into).collect());
        self
    }
}

impl UiComponentTemplate for UiTable {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_table(component, ctx)
    }
}
