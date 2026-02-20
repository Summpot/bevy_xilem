use bevy_ecs::{entity::Entity, prelude::Component, prelude::Resource};

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

/// Universal placement hints for floating overlays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OverlayPlacement {
    /// Centered inside the viewport.
    #[default]
    Center,
    /// Anchored above the anchor/window edge.
    Top,
    /// Anchored below the anchor/window edge.
    Bottom,
    /// Anchored to the left of the anchor/window edge.
    Left,
    /// Anchored to the right of the anchor/window edge.
    Right,
    /// Anchored to top edge, aligned to logical start.
    TopStart,
    /// Anchored to top edge, aligned to logical end.
    TopEnd,
    /// Anchored to bottom edge, aligned to logical start.
    BottomStart,
    /// Anchored to bottom edge, aligned to logical end.
    BottomEnd,
    /// Anchored to left edge, aligned to logical start.
    LeftStart,
    /// Anchored to right edge, aligned to logical start.
    RightStart,
}

/// Placement and collision behavior for an overlay entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayConfig {
    /// Preferred placement for this overlay.
    pub placement: OverlayPlacement,
    /// Anchor entity for placement. `None` anchors to the window.
    pub anchor: Option<Entity>,
    /// Enables automatic placement flipping when the preferred side overflows.
    pub auto_flip: bool,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            placement: OverlayPlacement::Center,
            anchor: None,
            auto_flip: false,
        }
    }
}

/// Runtime-computed window-space placement for an overlay surface.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct OverlayComputedPosition {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub placement: OverlayPlacement,
    /// Becomes `true` once layout/placement sync has written a valid final position.
    pub is_positioned: bool,
}

/// Centralized z-ordered overlay stack.
///
/// The last entry is the top-most overlay (highest z-index).
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlayStack {
    pub active_overlays: Vec<Entity>,
}

/// Behavioral state for an overlay instance.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayState {
    /// `true` for modal layers (dialogs/sheets) that block interactions under them.
    pub is_modal: bool,
    /// Optional trigger/anchor entity that opened this overlay.
    pub anchor: Option<Entity>,
}

/// Marker for overlays that should close on outside click.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AutoDismiss;

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
