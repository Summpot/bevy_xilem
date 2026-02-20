//! View helpers exported by `bevy_xilem`.
//!
//! This module exposes two naming groups:
//! - ECS-adapted controls (`button`, `button_with_child`, `checkbox`, `slider`, `switch`,
//!   `text_button`, `text_input`)
//! - Raw Xilem controls with `xilem_` prefix (`xilem_button`, `xilem_checkbox`, ...)
//!
//! # Example
//!
//! ```
//! use bevy_xilem::{
//!     button, xilem_button,
//!     bevy_ecs::world::World,
//!     xilem::view::label,
//! };
//!
//! let mut world = World::new();
//! let entity = world.spawn_empty().id();
//!
//! let _ecs_adapted = button(entity, (), "ECS event button");
//! let _raw_xilem = xilem_button::<(), (), _, _>(label("Raw xilem button"), |_| ());
//! ```
mod ecs_button_view;
mod ecs_control_views;
mod entity_scope_view;
mod opaque_hitbox_view;

pub use ecs_button_view::ecs_button as button;
pub use ecs_button_view::{EcsButtonView, ecs_button};
pub use ecs_control_views::ecs_button_with_child as button_with_child;
pub use ecs_control_views::ecs_checkbox as checkbox;
pub use ecs_control_views::ecs_slider as slider;
pub use ecs_control_views::ecs_switch as switch;
pub use ecs_control_views::ecs_text_button as text_button;
pub use ecs_control_views::ecs_text_input as text_input;
pub use ecs_control_views::{
    ecs_button_with_child, ecs_checkbox, ecs_slider, ecs_switch, ecs_text_button, ecs_text_input,
};
pub use entity_scope_view::entity_scope;
pub use opaque_hitbox_view::{OpaqueHitboxView, opaque_hitbox, opaque_hitbox_for_entity};
pub use xilem_masonry::view::{
    button as xilem_button, button_any_pointer as xilem_button_any_pointer,
    checkbox as xilem_checkbox, slider as xilem_slider, switch as xilem_switch,
    text_button as xilem_text_button, text_input as xilem_text_input,
};
