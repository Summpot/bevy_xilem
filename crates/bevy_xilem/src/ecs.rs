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

// ===== New Widget Extensions =====

// --- Radio Group ---

/// Radio button group component with multiple exclusive options.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiRadioGroup {
    /// Labels for each radio option.
    pub options: Vec<String>,
    /// Index of the currently selected option.
    pub selected: usize,
}

impl UiRadioGroup {
    #[must_use]
    pub fn new(options: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            options: options.into_iter().map(Into::into).collect(),
            selected: 0,
        }
    }

    #[must_use]
    pub fn with_selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

/// Emitted when the selection in a [`UiRadioGroup`] changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiRadioGroupChanged {
    pub group: Entity,
    pub selected: usize,
}

// --- Tabs ---

/// Tab bar component that shows labeled tabs and manages active content.
///
/// Place tab content entities as ECS children; the active tab index
/// controls which child is displayed.
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

// --- Tree View ---

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

// --- Table ---

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

// --- Menu Bar ---

/// A single item in a menu (inside a dropdown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiMenuItem {
    pub label: String,
    pub value: String,
}

impl UiMenuItem {
    #[must_use]
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

/// A top-level entry in a menu bar with a dropdown list of menu items.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiMenuBarItem {
    /// Label displayed on the menu bar button.
    pub label: String,
    /// Items shown in the dropdown panel.
    pub items: Vec<UiMenuItem>,
    /// Whether the dropdown is currently open.
    pub is_open: bool,
}

impl UiMenuBarItem {
    #[must_use]
    pub fn new(label: impl Into<String>, items: impl IntoIterator<Item = UiMenuItem>) -> Self {
        Self {
            label: label.into(),
            items: items.into_iter().collect(),
            is_open: false,
        }
    }
}

/// Marker for a horizontal menu bar container.
///
/// Place [`UiMenuBarItem`] entities as ECS children.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiMenuBar;

/// Floating menu item panel rendered in the overlay layer (one per open [`UiMenuBarItem`]).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiMenuItemPanel {
    /// The [`UiMenuBarItem`] anchor entity this panel belongs to.
    pub anchor: Entity,
}

/// Emitted when a menu item is selected from a [`UiMenuBarItem`] dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiMenuItemSelected {
    pub bar_item: Entity,
    pub value: String,
}

// --- Tooltip ---

/// Causes a floating tooltip to appear when the entity is hovered.
///
/// Works on any entity that emits hover interaction events (e.g. entities
/// projected as buttons).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct HasTooltip {
    /// Text shown inside the tooltip.
    pub text: String,
}

impl HasTooltip {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Floating tooltip overlay anchored to a source entity.
///
/// Spawned automatically by the tooltip system when an entity with
/// [`HasTooltip`] reports a hover-entered interaction event.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiTooltip {
    /// Tooltip body text.
    pub text: String,
    /// The entity that triggered this tooltip.
    pub anchor: Entity,
}

// --- Spinner ---

/// An animated loading spinner (indefinite progress indicator).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiSpinner {
    /// Optional label shown next to the spinner.
    pub label: Option<String>,
}

impl UiSpinner {
    #[must_use]
    pub fn new() -> Self {
        Self { label: None }
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Default for UiSpinner {
    fn default() -> Self {
        Self::new()
    }
}

// --- Color Picker ---

/// An inline color picker that opens an overlay panel for color selection.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiColorPicker {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// Whether the color picker overlay panel is currently open.
    pub is_open: bool,
}

impl UiColorPicker {
    #[must_use]
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self {
            r,
            g,
            b,
            is_open: false,
        }
    }
}

/// Floating color picker panel (rendered in the overlay layer).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiColorPickerPanel {
    /// The [`UiColorPicker`] anchor entity this panel belongs to.
    pub anchor: Entity,
}

/// Emitted when the selected color changes in a [`UiColorPicker`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiColorPickerChanged {
    pub picker: Entity,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

// --- Group Box ---

/// A titled group box that visually groups related content.
///
/// Place content entities as ECS children.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiGroupBox {
    /// Title displayed at the top of the group box.
    pub title: String,
}

impl UiGroupBox {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

// --- Split Pane ---

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

// --- Toast ---

/// Visual severity / colour of a [`UiToast`] notification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ToastKind {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

/// An auto-dismissing toast notification shown in the overlay corner.
///
/// Spawn this component (ideally under [`UiOverlayRoot`] via
/// [`crate::spawn_in_overlay_root`]) to display a toast message.
/// The built-in toast tick system automatically despawns it after
/// `duration_secs` seconds.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct UiToast {
    pub message: String,
    pub kind: ToastKind,
    /// Total display duration in seconds. 0.0 means it persists until manually dismissed.
    pub duration_secs: f32,
    /// Elapsed display time. Updated each frame by the toast tick system.
    pub elapsed_secs: f32,
}

impl UiToast {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Info,
            duration_secs: 3.0,
            elapsed_secs: 0.0,
        }
    }

    #[must_use]
    pub fn with_kind(mut self, kind: ToastKind) -> Self {
        self.kind = kind;
        self
    }

    #[must_use]
    pub fn with_duration(mut self, duration_secs: f32) -> Self {
        self.duration_secs = duration_secs;
        self
    }
}

// --- Date Picker ---

/// An inline date picker that opens a calendar overlay panel.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiDatePicker {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// Whether the calendar overlay panel is currently open.
    pub is_open: bool,
}

impl UiDatePicker {
    #[must_use]
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self {
            year,
            month: month.clamp(1, 12),
            day: day.clamp(1, 31),
            is_open: false,
        }
    }
}

/// Floating date picker calendar panel (rendered in the overlay layer).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiDatePickerPanel {
    /// The [`UiDatePicker`] anchor entity this panel belongs to.
    pub anchor: Entity,
    /// Month currently shown in the calendar (may differ from selected month).
    pub view_year: i32,
    pub view_month: u32,
}

/// Emitted when the selected date changes in a [`UiDatePicker`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiDatePickerChanged {
    pub picker: Entity,
    pub year: i32,
    pub month: u32,
    pub day: u32,
}
