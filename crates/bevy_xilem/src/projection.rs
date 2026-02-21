pub mod core;
pub mod dialog;
pub mod dropdown;
pub mod elements;
pub mod layout;
pub mod overlay;
pub mod utils;

pub use core::*;

use crate::ecs::{
    UiButton, UiComboBox, UiDialog, UiDropdownMenu, UiFlexColumn, UiFlexRow, UiLabel, UiOverlayRoot,
};

/// Register built-in projectors for built-in ECS demo components.
pub fn register_builtin_projectors(registry: &mut UiProjectorRegistry) {
    registry
        .register_component::<UiFlexColumn>(layout::project_flex_column)
        .register_component::<UiFlexRow>(layout::project_flex_row)
        .register_component::<UiLabel>(elements::project_label)
        .register_component::<UiButton>(elements::project_button)
        .register_component::<UiOverlayRoot>(overlay::project_overlay_root)
        .register_component::<UiDialog>(dialog::project_dialog)
        .register_component::<UiComboBox>(dropdown::project_combo_box)
        .register_component::<UiDropdownMenu>(dropdown::project_dropdown_menu);
}
