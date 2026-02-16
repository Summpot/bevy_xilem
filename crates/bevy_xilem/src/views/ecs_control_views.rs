use bevy_ecs::entity::Entity;
use masonry::core::{ArcStr, PointerButton};
use xilem_core::{Arg, MessageResult};
use xilem_masonry::view::{
    Button, Checkbox, Label, Slider, Switch, TextInput, button, checkbox, slider, switch,
    text_button, text_input,
};

use crate::events::emit_ui_action;

/// ECS-dispatching variant of `xilem_masonry::view::button`.
#[must_use]
pub fn ecs_button_with_child<A, V>(
    entity: Entity,
    action: A,
    child: V,
) -> Button<
    (),
    (),
    impl for<'a> Fn(Arg<'a, ()>, Option<PointerButton>) -> MessageResult<()> + Send + Sync + 'static,
    V,
>
where
    A: Clone + Send + Sync + 'static,
    V: xilem_masonry::WidgetView<(), ()>,
{
    button(child, move |_| {
        emit_ui_action(entity, action.clone());
    })
}

/// ECS-dispatching variant of `xilem_masonry::view::text_button`.
#[must_use]
pub fn ecs_text_button<A>(
    entity: Entity,
    action: A,
    text: impl Into<ArcStr>,
) -> Button<
    (),
    (),
    impl for<'a> Fn(Arg<'a, ()>, Option<PointerButton>) -> MessageResult<()> + Send + Sync + 'static,
    Label,
>
where
    A: Clone + Send + Sync + 'static,
{
    text_button(text, move |_| {
        emit_ui_action(entity, action.clone());
    })
}

/// ECS-dispatching variant of `xilem_masonry::view::checkbox`.
#[must_use]
pub fn ecs_checkbox<A, F>(
    entity: Entity,
    label: impl Into<ArcStr>,
    checked: bool,
    map_action: F,
) -> Checkbox<(), (), impl for<'a> Fn(Arg<'a, ()>, bool) -> () + Send + Sync + 'static>
where
    A: Send + Sync + 'static,
    F: Fn(bool) -> A + Send + Sync + 'static,
{
    checkbox(label, checked, move |_, value| {
        emit_ui_action(entity, map_action(value));
    })
}

/// ECS-dispatching variant of `xilem_masonry::view::slider`.
#[must_use]
pub fn ecs_slider<A, F>(
    entity: Entity,
    min: f64,
    max: f64,
    value: f64,
    map_action: F,
) -> Slider<(), (), impl for<'a> Fn(Arg<'a, ()>, f64) -> () + Send + Sync + 'static>
where
    A: Send + Sync + 'static,
    F: Fn(f64) -> A + Send + Sync + 'static,
{
    slider(min, max, value, move |_, value| {
        emit_ui_action(entity, map_action(value));
    })
}

/// ECS-dispatching variant of `xilem_masonry::view::switch`.
#[must_use]
pub fn ecs_switch<A, F>(
    entity: Entity,
    on: bool,
    map_action: F,
) -> Switch<(), (), impl for<'a> Fn(Arg<'a, ()>, bool) -> () + Send + Sync + 'static>
where
    A: Send + Sync + 'static,
    F: Fn(bool) -> A + Send + Sync + 'static,
{
    switch(on, move |_, value| {
        emit_ui_action(entity, map_action(value));
    })
}

/// ECS-dispatching variant of `xilem_masonry::view::text_input`.
#[must_use]
pub fn ecs_text_input<A, F>(entity: Entity, contents: String, map_action: F) -> TextInput<(), ()>
where
    A: Send + Sync + 'static,
    F: Fn(String) -> A + Send + Sync + 'static,
{
    text_input(contents, move |_, value| {
        emit_ui_action(entity, map_action(value));
    })
}
