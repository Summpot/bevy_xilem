use bevy_ecs::prelude::*;

use crate::{ProjectionCtx, UiView, controls::UiControlTemplate};

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

impl UiControlTemplate for UiTable {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_table(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Type("UiTable"),
      setter: (
        layout: (
          gap: 1.0,
          border_width: 1.0,
          corner_radius: 6.0,
        ),
        colors: (
          border: Hex("#3F3F3F"),
        ),
      ),
    ),
    (
      selector: Class("widget.table.header"),
      setter: (
        layout: (
          padding: 6.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#2A2A2A"),
          border: Hex("#3F3F3F"),
          text: Hex("#F3F3F3"),
        ),
      ),
    ),
    (
      selector: Class("widget.table.cell"),
      setter: (
        layout: (
          padding: 6.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#242424"),
          border: Hex("#333333"),
          text: Hex("#E0E0E0"),
        ),
      ),
    ),
  ],
)
"##
    }
}
