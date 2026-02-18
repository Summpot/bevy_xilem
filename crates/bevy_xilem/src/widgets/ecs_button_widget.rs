use std::any::TypeId;

use bevy_ecs::entity::Entity;
use masonry::{
    accesskit::{Node, Role},
    core::keyboard::{Key, NamedKey},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, HasProperty, LayoutCtx, MeasureCtx,
        PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef,
        Property, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
    },
    kurbo::Size,
    layout::{LayoutSize, LenReq, SizeDef},
    properties::{Background, BorderColor, BorderWidth, ContentColor, CornerRadius, Padding},
    widgets::Label,
};
use vello::Scene;

use crate::{
    events::{UiEvent, push_global_ui_event},
    styling::UiInteractionEvent,
};

/// Internal action used to force Xilem driver ticks for ECS button state changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcsButtonWidgetAction {
    StateChanged,
}

/// Masonry widget that emits typed ECS actions without user-facing closures.
pub struct EcsButtonWidget<A> {
    entity: Entity,
    action: A,
    label: WidgetPod<Label>,
    hovered: bool,
    pressed: bool,
}

impl<A> HasProperty<ContentColor> for EcsButtonWidget<A> {}

impl<A> EcsButtonWidget<A> {
    #[must_use]
    pub fn new(entity: Entity, action: A, label: impl Into<masonry::core::ArcStr>) -> Self {
        Self {
            entity,
            action,
            label: Label::new(label).with_auto_id().to_pod(),
            hovered: false,
            pressed: false,
        }
    }

    #[must_use]
    pub const fn entity(&self) -> Entity {
        self.entity
    }
}

impl<A> EcsButtonWidget<A>
where
    A: Clone + Send + Sync + 'static,
{
    pub fn set_entity(this: &mut WidgetMut<'_, Self>, entity: Entity) {
        this.widget.entity = entity;
    }

    pub fn set_action(this: &mut WidgetMut<'_, Self>, action: A) {
        this.widget.action = action;
    }

    pub fn set_label(this: &mut WidgetMut<'_, Self>, label: impl Into<masonry::core::ArcStr>) {
        Label::set_text(&mut this.ctx.get_mut(&mut this.widget.label), label);
    }

    fn push_action(&self) {
        push_global_ui_event(UiEvent::typed(self.entity, self.action.clone()));
    }

    fn push_interaction(&self, event: UiInteractionEvent) {
        push_global_ui_event(UiEvent::typed(self.entity, event));
    }

    fn set_hovered(&mut self, hovered: bool) -> bool {
        if self.hovered != hovered {
            self.hovered = hovered;
            self.push_interaction(if hovered {
                UiInteractionEvent::PointerEntered
            } else {
                UiInteractionEvent::PointerLeft
            });
            true
        } else {
            false
        }
    }

    fn set_pressed(&mut self, pressed: bool) -> bool {
        if self.pressed != pressed {
            self.pressed = pressed;
            self.push_interaction(if pressed {
                UiInteractionEvent::PointerPressed
            } else {
                UiInteractionEvent::PointerReleased
            });
            true
        } else {
            false
        }
    }
}

impl<A> Widget for EcsButtonWidget<A>
where
    A: Clone + Send + Sync + 'static,
{
    type Action = EcsButtonWidgetAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(..) => {
                ctx.request_focus();
                ctx.capture_pointer();
                ctx.request_render();
            }
            PointerEvent::Up(PointerButtonEvent { button, .. }) => {
                if matches!(button, Some(PointerButton::Primary))
                    && ctx.is_active()
                    && ctx.is_hovered()
                {
                    self.push_action();
                    ctx.submit_action::<Self::Action>(EcsButtonWidgetAction::StateChanged);
                }
                ctx.request_render();
            }
            PointerEvent::Move(..) | PointerEvent::Leave(..) => {}
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && (matches!(&event.key, Key::Character(c) if c == " ")
                || event.key == Key::Named(NamedKey::Enter))
        {
            self.push_action();
            ctx.submit_action::<Self::Action>(EcsButtonWidgetAction::StateChanged);
            ctx.request_render();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if matches!(event.action, masonry::accesskit::Action::Click) {
            self.push_action();
            ctx.submit_action::<Self::Action>(EcsButtonWidgetAction::StateChanged);
            ctx.request_render();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.label);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::HoveredChanged(hovered) => {
                if self.set_hovered(*hovered) {
                    ctx.request_render();
                }
            }
            Update::ActiveChanged(active) => {
                if self.set_pressed(*active) {
                    ctx.request_render();
                }
            }
            Update::DisabledChanged(true) => {
                let hover_changed = self.set_hovered(false);
                let pressed_changed = self.set_pressed(false);
                if hover_changed || pressed_changed {
                    ctx.request_render();
                }
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if Padding::matches(property_type) || BorderWidth::matches(property_type) {
            ctx.request_layout();
            ctx.request_render();
            return;
        }

        if ContentColor::matches(property_type)
            || CornerRadius::matches(property_type)
            || BorderColor::matches(property_type)
            || Background::matches(property_type)
        {
            ctx.request_render();
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: masonry::kurbo::Axis,
        len_req: LenReq,
        cross_length: Option<f64>,
    ) -> f64 {
        let auto_length = len_req.into();
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);

        ctx.compute_length(
            &mut self.label,
            auto_length,
            context_size,
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let child_size = ctx.compute_size(&mut self.label, SizeDef::fit(size), size.into());
        ctx.run_layout(&mut self.label, child_size);

        let child_origin = ((size - child_size).to_vec2() * 0.5).to_point();
        ctx.place_child(&mut self.label, child_origin);

        let child_baseline = ctx.child_baseline_offset(&self.label);
        let child_bottom = child_origin.y + child_size.height;
        let bottom_gap = size.height - child_bottom;
        ctx.set_baseline_offset(child_baseline + bottom_gap);
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _scene: &mut Scene) {}

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.add_action(masonry::accesskit::Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.label.id()])
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(format!("entity={}", self.entity.to_bits()))
    }
}
