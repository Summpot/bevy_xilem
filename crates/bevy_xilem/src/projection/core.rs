use bevy_ecs::prelude::*;
use std::{fmt, marker::PhantomData, sync::Arc};
use xilem_masonry::AnyWidgetView;

/// Xilem state used by synthesized UI views.
pub type UiXilemState = ();
/// Xilem action type used by synthesized UI views.
pub type UiXilemAction = ();

/// Type-erased Xilem Masonry view used as projection output.
pub type UiAnyView = AnyWidgetView<UiXilemState, UiXilemAction>;
/// Shared synthesized view handle.
pub type UiView = Arc<UiAnyView>;

/// Built-in button action emitted by [`UiButton`] projector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinUiAction {
    Clicked,
}

/// Projection context passed to projector implementations.
pub struct ProjectionCtx<'a> {
    pub world: &'a World,
    pub entity: Entity,
    pub node_id: u64,
    pub children: Vec<UiView>,
}

impl fmt::Debug for ProjectionCtx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjectionCtx")
            .field("entity", &self.entity)
            .field("node_id", &self.node_id)
            .field("children_len", &self.children.len())
            .finish()
    }
}

/// Maps ECS entity data into a concrete Xilem Masonry view.
pub trait UiProjector: Send + Sync + 'static {
    fn project(&self, ctx: ProjectionCtx<'_>) -> Option<UiView>;
}

struct ComponentProjector<C: Component> {
    projector: fn(&C, ProjectionCtx<'_>) -> UiView,
    _marker: PhantomData<C>,
}

impl<C: Component> UiProjector for ComponentProjector<C> {
    fn project(&self, ctx: ProjectionCtx<'_>) -> Option<UiView> {
        let component = ctx.world.get::<C>(ctx.entity)?;
        Some((self.projector)(component, ctx))
    }
}

/// Registry of projector implementations.
#[derive(Resource, Default)]
pub struct UiProjectorRegistry {
    projectors: Vec<Box<dyn UiProjector>>,
}

impl UiProjectorRegistry {
    /// Register a raw projector implementation.
    pub fn register_projector<P: UiProjector>(&mut self, projector: P) -> &mut Self {
        self.projectors.push(Box::new(projector));
        self
    }

    /// Register a projector bound to a specific ECS component type.
    pub fn register_component<C: Component>(
        &mut self,
        projector: fn(&C, ProjectionCtx<'_>) -> UiView,
    ) -> &mut Self {
        self.register_projector(ComponentProjector::<C> {
            projector,
            _marker: PhantomData,
        })
    }

    pub(crate) fn project_node(
        &self,
        world: &World,
        entity: Entity,
        node_id: u64,
        children: Vec<UiView>,
    ) -> Option<UiView> {
        // Last registered projector wins.
        for projector in self.projectors.iter().rev() {
            let ctx = ProjectionCtx {
                world,
                entity,
                node_id,
                children: children.clone(),
            };
            if let Some(view) = projector.project(ctx) {
                return Some(view);
            }
        }

        None
    }
}
