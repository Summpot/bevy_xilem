use bevy_ecs::{entity::Entity, prelude::Component};

/// Marker component for UI tree roots.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct UiRoot;

/// Marker component for the global overlay/portal root.
///
/// Overlay entities (dialogs, dropdowns, tooltips, etc.) should be attached as
/// descendants of this node so they are not clipped by regular layout parents.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct UiOverlayRoot;

/// Built-in vertical container marker.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiFlexColumn;

/// Built-in horizontal container marker.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiFlexRow;

/// Built-in text label component.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiLabel {
    pub text: String,
}

impl UiLabel {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Translation key marker for localized text projection.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct LocalizeText {
    pub key: String,
}

impl LocalizeText {
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }
}

/// Built-in button component.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiButton {
    pub label: String,
}

impl UiButton {
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

/// Modal dialog entity projected in the overlay layer.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiDialog {
    pub title: String,
    pub body: String,
    pub dismiss_label: String,
    pub title_key: Option<String>,
    pub body_key: Option<String>,
    pub dismiss_key: Option<String>,
}

impl UiDialog {
    #[must_use]
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            dismiss_label: "Close".to_string(),
            title_key: None,
            body_key: None,
            dismiss_key: None,
        }
    }

    #[must_use]
    pub fn with_localized_keys(
        mut self,
        title_key: impl Into<String>,
        body_key: impl Into<String>,
        dismiss_key: impl Into<String>,
    ) -> Self {
        self.title_key = Some(title_key.into());
        self.body_key = Some(body_key.into());
        self.dismiss_key = Some(dismiss_key.into());
        self
    }
}

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

/// Preferred dropdown placement around an anchor control.
///
/// This supports eight practical directions:
/// - vertical: top / bottom with start-center-end alignment
/// - horizontal: left / right with start alignment
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UiDropdownPlacement {
    #[default]
    BottomStart,
    Bottom,
    BottomEnd,
    TopStart,
    Top,
    TopEnd,
    RightStart,
    LeftStart,
}

/// Combo-box anchor control.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiComboBox {
    pub options: Vec<UiComboOption>,
    pub selected: usize,
    pub is_open: bool,
    pub placeholder: String,
    pub placeholder_key: Option<String>,
    pub dropdown_placement: UiDropdownPlacement,
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
            dropdown_placement: UiDropdownPlacement::BottomStart,
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
    pub fn with_dropdown_placement(mut self, placement: UiDropdownPlacement) -> Self {
        self.dropdown_placement = placement;
        self
    }

    #[must_use]
    pub fn with_auto_flip_placement(mut self, auto_flip: bool) -> Self {
        self.auto_flip_placement = auto_flip;
        self
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

/// Marker telling an overlay widget which anchor entity it follows.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchoredTo(pub Entity);

/// Cached window-space rectangle for anchored overlays.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct OverlayAnchorRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// Emitted when a [`UiComboBox`] selection changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiComboBoxChanged {
    pub combo: Entity,
    pub selected: usize,
    pub value: String,
}
