pub mod core;
pub mod dialog;
pub mod dropdown;
pub mod elements;
pub mod layout;
pub mod overlay;
pub mod utils;
pub mod widgets;

pub use core::*;

use crate::ecs::{
    UiButton, UiCheckbox, UiColorPicker, UiColorPickerPanel, UiComboBox, UiDatePicker,
    UiDatePickerPanel, UiDialog, UiDropdownMenu, UiFlexColumn, UiFlexRow, UiGroupBox, UiLabel,
    UiMenuBar, UiMenuBarItem, UiMenuItemPanel, UiOverlayRoot, UiRadioGroup, UiSlider, UiSpinner,
    UiSplitPane, UiSwitch, UiTabBar, UiTable, UiTextInput, UiToast, UiTooltip, UiTreeNode,
};

/// Register non-control foundational projectors.
pub fn register_core_projectors(registry: &mut UiProjectorRegistry) {
    registry
        .register_component::<UiFlexColumn>(layout::project_flex_column)
        .register_component::<UiFlexRow>(layout::project_flex_row)
        .register_component::<UiLabel>(elements::project_label)
        .register_component::<UiOverlayRoot>(overlay::project_overlay_root);
}

/// Register built-in projectors for built-in ECS demo components.
///
/// Compatibility helper: controls are now expected to be registered through
/// `AppBevyXilemExt::register_ui_control::<T>()`.
pub fn register_builtin_projectors(registry: &mut UiProjectorRegistry) {
    register_core_projectors(registry);

    registry
        .register_component::<UiButton>(elements::project_button)
        .register_component::<UiCheckbox>(elements::project_checkbox)
        .register_component::<UiSlider>(elements::project_slider)
        .register_component::<UiSwitch>(elements::project_switch)
        .register_component::<UiTextInput>(elements::project_text_input)
        .register_component::<UiDialog>(dialog::project_dialog)
        .register_component::<UiComboBox>(dropdown::project_combo_box)
        .register_component::<UiDropdownMenu>(dropdown::project_dropdown_menu)
        .register_component::<UiRadioGroup>(widgets::project_radio_group)
        .register_component::<UiTabBar>(widgets::project_tab_bar)
        .register_component::<UiTreeNode>(widgets::project_tree_node)
        .register_component::<UiTable>(widgets::project_table)
        .register_component::<UiMenuBar>(widgets::project_menu_bar)
        .register_component::<UiMenuBarItem>(widgets::project_menu_bar_item)
        .register_component::<UiMenuItemPanel>(widgets::project_menu_item_panel)
        .register_component::<UiTooltip>(widgets::project_tooltip)
        .register_component::<UiSpinner>(widgets::project_spinner)
        .register_component::<UiColorPicker>(widgets::project_color_picker)
        .register_component::<UiColorPickerPanel>(widgets::project_color_picker_panel)
        .register_component::<UiGroupBox>(widgets::project_group_box)
        .register_component::<UiSplitPane>(widgets::project_split_pane)
        .register_component::<UiToast>(widgets::project_toast)
        .register_component::<UiDatePicker>(widgets::project_date_picker)
        .register_component::<UiDatePickerPanel>(widgets::project_date_picker_panel);
}
