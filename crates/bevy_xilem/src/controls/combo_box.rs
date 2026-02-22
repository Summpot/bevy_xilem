use bevy_ecs::{entity::Entity, prelude::*};

use crate::{
    OverlayPlacement, ProjectionCtx, StyleClass, UiLabel, UiView, controls::UiControlTemplate,
    templates::ensure_template_part,
};

/// Single combo option entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiComboOption {
    pub value: String,
    pub label: String,
    pub label_key: Option<String>,
}

impl UiComboOption {
    #[must_use]
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            label_key: None,
        }
    }

    #[must_use]
    pub fn with_label_key(mut self, key: impl Into<String>) -> Self {
        self.label_key = Some(key.into());
        self
    }
}

/// Backward-compatible alias for overlay placement in combo APIs.
pub type UiDropdownPlacement = OverlayPlacement;

/// Combo-box anchor control.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiComboBox {
    pub options: Vec<UiComboOption>,
    pub selected: usize,
    pub is_open: bool,
    pub placeholder: String,
    pub placeholder_key: Option<String>,
    pub dropdown_placement: OverlayPlacement,
    pub auto_flip_placement: bool,
}

impl UiComboBox {
    #[must_use]
    pub fn new(options: Vec<UiComboOption>) -> Self {
        Self {
            options,
            selected: 0,
            is_open: false,
            placeholder: "Select".to_string(),
            placeholder_key: None,
            dropdown_placement: OverlayPlacement::BottomStart,
            auto_flip_placement: true,
        }
    }

    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    #[must_use]
    pub fn with_placeholder_key(mut self, key: impl Into<String>) -> Self {
        self.placeholder_key = Some(key.into());
        self
    }

    #[must_use]
    pub fn with_dropdown_placement(mut self, placement: OverlayPlacement) -> Self {
        self.dropdown_placement = placement;
        self
    }

    #[must_use]
    pub fn with_overlay_placement(self, placement: OverlayPlacement) -> Self {
        self.with_dropdown_placement(placement)
    }

    #[must_use]
    pub fn with_auto_flip_placement(mut self, auto_flip: bool) -> Self {
        self.auto_flip_placement = auto_flip;
        self
    }

    #[must_use]
    pub fn with_overlay_auto_flip(self, auto_flip: bool) -> Self {
        self.with_auto_flip_placement(auto_flip)
    }

    #[must_use]
    pub fn clamped_selected(&self) -> Option<usize> {
        if self.options.is_empty() {
            None
        } else {
            Some(self.selected.min(self.options.len() - 1))
        }
    }
}

/// Floating dropdown list entity rendered in the overlay layer.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiDropdownMenu;

/// Emitted when a [`UiComboBox`] selection changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiComboBoxChanged {
    pub combo: Entity,
    pub selected: usize,
    pub value: String,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartComboBoxDisplay;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartComboBoxChevron;

impl UiControlTemplate for UiComboBox {
    fn expand(world: &mut World, entity: Entity) {
        let combo = world.get::<UiComboBox>(entity).cloned();
        let Some(combo) = combo else {
            return;
        };

        let display = combo
            .clamped_selected()
            .and_then(|index| combo.options.get(index))
            .map(|opt| opt.label.clone())
            .unwrap_or(combo.placeholder.clone());
        let chevron = if combo.is_open { "▴" } else { "▾" };

        let display_part = ensure_template_part::<PartComboBoxDisplay, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.combo_box.display".to_string()]),
            )
        });
        let chevron_part = ensure_template_part::<PartComboBoxChevron, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.combo_box.chevron".to_string()]),
            )
        });

        if let Some(mut label) = world.get_mut::<UiLabel>(display_part) {
            label.text = display;
        }
        if let Some(mut label) = world.get_mut::<UiLabel>(chevron_part) {
            label.text = chevron.to_string();
        }
    }

    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::dropdown::project_combo_box(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Class("overlay.dropdown.menu"),
      setter: (
        layout: (
          padding: 8.0,
          corner_radius: 10.0,
          border_width: 1.0,
          gap: 6.0,
        ),
        colors: (
          bg: Hex("#161C2A"),
          border: Hex("#384664"),
        ),
      ),
    ),
    (
      selector: Class("overlay.dropdown.item"),
      setter: (
        layout: (
          padding: 6.0,
          corner_radius: 6.0,
        ),
        text: (
          size: 15.0,
        ),
        colors: (
          text: Hex("#DCE7FF"),
          hover_bg: Hex("#22314E"),
          pressed_bg: Hex("#1A2940"),
        ),
      ),
    ),
  ],
)
"##
    }
}

impl UiControlTemplate for UiDropdownMenu {
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::dropdown::project_dropdown_menu(component, ctx)
    }
}
