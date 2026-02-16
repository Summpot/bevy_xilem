use std::any::TypeId;

use bevy_ecs::entity::Entity;
use masonry::{
    accesskit::{Node, Role},
    core::keyboard::{Key, NamedKey},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NoAction, PaintCtx,
        PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
        TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
    },
    kurbo::Size,
    layout::{LayoutSize, LenReq, SizeDef},
    widgets::Label,
};
use vello::Scene;

use crate::{
    events::{UiEvent, push_global_ui_event},
    styling::UiInteractionEvent,
};

/// Masonry widget that emits typed ECS actions without user-facing closures.
pub struct EcsButtonWidget<A> {
    entity: Entity,
    action: A,
    label: WidgetPod<Label>,
    hovered: bool,
    pressed: bool,
}

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

    fn set_hovered(&mut self, hovered: bool) {
        if self.hovered != hovered {
            self.hovered = hovered;
            self.push_interaction(if hovered {
                UiInteractionEvent::PointerEntered
            } else {
                UiInteractionEvent::PointerLeft
            });
        }
    }

    fn set_pressed(&mut self, pressed: bool) {
        if self.pressed != pressed {
            self.pressed = pressed;
            self.push_interaction(if pressed {
                UiInteractionEvent::PointerPressed
            } else {
                UiInteractionEvent::PointerReleased
            });
        }
    }
}

impl<A> Widget for EcsButtonWidget<A>
where
    A: Clone + Send + Sync + 'static,
{
    type Action = NoAction;

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
                self.set_hovered(ctx.is_hovered());
                self.set_pressed(true);
                ctx.request_paint_only();
            }
            PointerEvent::Up(PointerButtonEvent { button, .. }) => {
                if matches!(button, Some(PointerButton::Primary))
                    && ctx.is_active()
                    && ctx.is_hovered()
                {
                    self.push_action();
                }
                self.set_pressed(false);
                self.set_hovered(ctx.is_hovered());
                ctx.request_paint_only();
            }
            PointerEvent::Move(..) => {
                self.set_hovered(ctx.is_hovered());
            }
            PointerEvent::Leave(..) => {
                self.set_hovered(false);
                self.set_pressed(false);
            }
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
            ctx.request_paint_only();
        }
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if matches!(event.action, masonry::accesskit::Action::Click) {
            self.push_action();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.label);
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: TypeId) {}

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
}
