use std::{any::TypeId, collections::HashSet};

use bevy_app::App;
use bevy_ecs::prelude::*;

use crate::{AppBevyXilemExt, ProjectionCtx, StyleTypeRegistry, UiView};

mod button;
mod checkbox;
mod color_picker;
mod combo_box;
mod date_picker;
mod dialog;
mod group_box;
mod menu;
mod radio_group;
mod slider;
mod spinner;
mod split_pane;
mod switch;
mod tab_bar;
mod table;
mod text_input;
mod toast;
mod tooltip;
mod tree_node;

pub use button::*;
pub use checkbox::*;
pub use color_picker::*;
pub use combo_box::*;
pub use date_picker::*;
pub use dialog::*;
pub use group_box::*;
pub use menu::*;
pub use radio_group::*;
pub use slider::*;
pub use spinner::*;
pub use split_pane::*;
pub use switch::*;
pub use tab_bar::*;
pub use table::*;
pub use text_input::*;
pub use toast::*;
pub use tooltip::*;
pub use tree_node::*;

/// Unified contract for ECS-native UI controls.
///
/// A control owns:
/// - one-time ECS expansion into template parts (`expand`),
/// - projection from ECS state into a retained Masonry view (`project`),
/// - baseline fallback style rules (`default_style_ron`).
pub trait UiControlTemplate: Component + Sized {
    /// Expand a newly-spawned logical control entity into child template parts.
    fn expand(_world: &mut World, _entity: Entity) {}

    /// Project this control into a Masonry view.
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView;

    /// Component-local fallback style rules in stylesheet RON form.
    fn default_style_ron() -> &'static str {
        ""
    }

    /// Register selector type aliases used by this control's fallback style.
    fn register_style_types(registry: &mut StyleTypeRegistry) {
        registry.register_type_aliases::<Self>();
    }
}

/// Internal resource tracking which control types were already registered.
#[derive(Resource, Debug, Default)]
pub struct RegisteredUiControlTypes {
    seen: HashSet<TypeId>,
}

impl RegisteredUiControlTypes {
    pub fn insert<T: 'static>(&mut self) -> bool {
        self.seen.insert(TypeId::of::<T>())
    }
}

/// Generic expansion system for any [`UiControlTemplate`].
///
/// Runs only for entities where the control component was just added.
pub fn expand_added_ui_control_templates<T: UiControlTemplate>(world: &mut World) {
    let entities = {
        let mut query = world.query_filtered::<Entity, Added<T>>();
        query.iter(world).collect::<Vec<_>>()
    };

    for entity in entities {
        if world.get_entity(entity).is_ok() {
            T::expand(world, entity);
        }
    }
}

/// Compatibility helper that expands all entities carrying `T`, not only `Added<T>`.
pub fn expand_all_ui_control_templates<T: UiControlTemplate>(world: &mut World) {
    let entities = {
        let mut query = world.query_filtered::<Entity, With<T>>();
        query.iter(world).collect::<Vec<_>>()
    };

    for entity in entities {
        if world.get_entity(entity).is_ok() {
            T::expand(world, entity);
        }
    }
}

/// Register all built-in controls with the unified control API.
pub fn register_builtin_ui_controls(app: &mut App) {
    app.register_ui_control::<button::UiButton>()
        .register_ui_control::<checkbox::UiCheckbox>()
        .register_ui_control::<slider::UiSlider>()
        .register_ui_control::<switch::UiSwitch>()
        .register_ui_control::<text_input::UiTextInput>()
        .register_ui_control::<dialog::UiDialog>()
        .register_ui_control::<combo_box::UiComboBox>()
        .register_ui_control::<combo_box::UiDropdownMenu>()
        .register_ui_control::<radio_group::UiRadioGroup>()
        .register_ui_control::<tab_bar::UiTabBar>()
        .register_ui_control::<tree_node::UiTreeNode>()
        .register_ui_control::<table::UiTable>()
        .register_ui_control::<menu::UiMenuBar>()
        .register_ui_control::<menu::UiMenuBarItem>()
        .register_ui_control::<menu::UiMenuItemPanel>()
        .register_ui_control::<tooltip::UiTooltip>()
        .register_ui_control::<spinner::UiSpinner>()
        .register_ui_control::<color_picker::UiColorPicker>()
        .register_ui_control::<color_picker::UiColorPickerPanel>()
        .register_ui_control::<group_box::UiGroupBox>()
        .register_ui_control::<split_pane::UiSplitPane>()
        .register_ui_control::<toast::UiToast>()
        .register_ui_control::<date_picker::UiDatePicker>()
        .register_ui_control::<date_picker::UiDatePickerPanel>();
}
