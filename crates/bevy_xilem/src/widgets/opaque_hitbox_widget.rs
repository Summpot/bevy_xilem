use std::any::TypeId;

use bevy_ecs::entity::Entity;
use masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PointerEvent,
        PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, UpdateCtx, Widget, WidgetId,
        WidgetMut, WidgetPod, WidgetRef,
    },
    kurbo::{Axis, Point, Size},
    layout::LenReq,
};
use vello::Scene;

/// Pointer-opaque wrapper that forces hit-testing across its full layout bounds.
///
/// This widget is intentionally paint-transparent but pointer-solid.
pub struct OpaqueHitboxWidget {
    entity: Option<Entity>,
    child: WidgetPod<dyn Widget>,
}

impl OpaqueHitboxWidget {
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            entity: None,
            child: child.erased().to_pod(),
        }
    }

    #[must_use]
    pub fn new_for_entity(entity: Entity, child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            entity: Some(entity),
            child: child.erased().to_pod(),
        }
    }

    pub fn set_entity(this: &mut WidgetMut<'_, Self>, entity: Option<Entity>) {
        this.widget.entity = entity;
    }

    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for OpaqueHitboxWidget {
    type Action = ();

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &PointerEvent,
    ) {
        // Ensure this wrapper acts as a physical pointer backplane.
        ctx.set_handled();
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<f64>,
    ) -> f64 {
        ctx.redirect_measurement(&mut self.child, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        let child_baseline = ctx.child_baseline_offset(&self.child);
        ctx.set_baseline_offset(child_baseline);
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _scene: &mut Scene) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn find_widget_under_pointer<'c>(
        &'c self,
        ctx: QueryCtx<'c>,
        pos: Point,
    ) -> Option<WidgetRef<'c, dyn Widget>> {
        if ctx.is_stashed() {
            return None;
        }

        let local_pos = ctx.window_transform().inverse() * pos;

        if let Some(clip) = ctx.clip_path()
            && !clip.contains(local_pos)
        {
            return None;
        }

        for child_id in self.children_ids().iter().rev() {
            let child_ref = ctx.get(*child_id);
            if let Some(child) = child_ref.find_widget_under_pointer(pos) {
                return Some(child);
            }
        }

        if ctx.accepts_pointer_interaction() && ctx.border_box().contains(local_pos) {
            Some(ctx.get(self.child.id()))
        } else {
            None
        }
    }

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        self.entity
            .map(|entity| format!("opaque_hitbox_entity={}", entity.to_bits()))
    }

    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!(
            "OpaqueHitboxWidget",
            id = id.trace(),
            entity = self.entity.map(|entity| entity.to_bits())
        )
    }
}
