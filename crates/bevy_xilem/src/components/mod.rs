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
mod scroll_view;
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
pub use scroll_view::*;
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

/// Unified contract for ECS-native UI components.
///
/// A UI component owns:
/// - one-time ECS expansion into template parts (`expand`),
/// - projection from ECS state into a retained Masonry view (`project`).
pub trait UiComponentTemplate: Component + Sized {
    /// Expand a newly-spawned logical UI component entity into child template parts.
    fn expand(_world: &mut World, _entity: Entity) {}

    /// Project this UI component into a Masonry view.
    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView;

    /// Register selector type aliases used by this UI component.
    fn register_style_types(registry: &mut StyleTypeRegistry) {
        registry.register_type_aliases::<Self>();
    }
}

/// Implement [`UiComponentTemplate`] for a component by forwarding to a projector function.
///
/// This is intended for application/example-defined ECS components that already expose
/// a projector function with signature `fn(&T, ProjectionCtx<'_>) -> UiView`.
#[macro_export]
macro_rules! impl_ui_component_template {
    ($component:ty, $project:path $(,)?) => {
        impl $crate::UiComponentTemplate for $component {
            fn project(component: &Self, ctx: $crate::ProjectionCtx<'_>) -> $crate::UiView {
                $project(component, ctx)
            }
        }
    };
}

/// Internal resource tracking which UI component types were already registered.
#[derive(Resource, Debug, Default)]
pub struct RegisteredUiComponentTypes {
    seen: HashSet<TypeId>,
}

impl RegisteredUiComponentTypes {
    pub fn insert<T: 'static>(&mut self) -> bool {
        self.seen.insert(TypeId::of::<T>())
    }
}

/// Generic expansion system for any [`UiComponentTemplate`].
///
/// Runs only for entities where the UI component was just added.
pub fn expand_added_ui_component_templates<T: UiComponentTemplate>(world: &mut World) {
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
pub fn expand_all_ui_component_templates<T: UiComponentTemplate>(world: &mut World) {
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

/// Register all built-in UI components with the unified UI component API.
pub fn register_builtin_ui_components(app: &mut App) {
    app.register_ui_component::<button::UiButton>()
        .register_ui_component::<checkbox::UiCheckbox>()
        .register_ui_component::<slider::UiSlider>()
        .register_ui_component::<switch::UiSwitch>()
        .register_ui_component::<text_input::UiTextInput>()
        .register_ui_component::<dialog::UiDialog>()
        .register_ui_component::<combo_box::UiComboBox>()
        .register_ui_component::<combo_box::UiDropdownMenu>()
        .register_ui_component::<radio_group::UiRadioGroup>()
        .register_ui_component::<scroll_view::UiScrollView>()
        .register_ui_component::<tab_bar::UiTabBar>()
        .register_ui_component::<tree_node::UiTreeNode>()
        .register_ui_component::<table::UiTable>()
        .register_ui_component::<menu::UiMenuBar>()
        .register_ui_component::<menu::UiMenuBarItem>()
        .register_ui_component::<menu::UiMenuItemPanel>()
        .register_ui_component::<tooltip::UiTooltip>()
        .register_ui_component::<spinner::UiSpinner>()
        .register_ui_component::<color_picker::UiColorPicker>()
        .register_ui_component::<color_picker::UiColorPickerPanel>()
        .register_ui_component::<group_box::UiGroupBox>()
        .register_ui_component::<split_pane::UiSplitPane>()
        .register_ui_component::<toast::UiToast>()
        .register_ui_component::<date_picker::UiDatePicker>()
        .register_ui_component::<date_picker::UiDatePickerPanel>();
}
