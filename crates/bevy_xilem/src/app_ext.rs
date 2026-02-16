use bevy_app::App;
use bevy_ecs::prelude::Component;

use crate::{ProjectionCtx, UiProjector, UiProjectorRegistry, UiView};

/// Fluent extension methods for registering bevy_xilem projectors on a Bevy [`App`].
pub trait AppBevyXilemExt {
    /// Register a typed projector for a specific component.
    fn register_projector<C: Component>(
        &mut self,
        projector: fn(&C, ProjectionCtx<'_>) -> UiView,
    ) -> &mut Self;

    /// Register a raw projector implementation.
    fn register_raw_projector<P: UiProjector>(&mut self, projector: P) -> &mut Self;
}

impl AppBevyXilemExt for App {
    fn register_projector<C: Component>(
        &mut self,
        projector: fn(&C, ProjectionCtx<'_>) -> UiView,
    ) -> &mut Self {
        self.init_resource::<UiProjectorRegistry>();
        self.world_mut()
            .resource_mut::<UiProjectorRegistry>()
            .register_component::<C>(projector);
        self
    }

    fn register_raw_projector<P: UiProjector>(&mut self, projector: P) -> &mut Self {
        self.init_resource::<UiProjectorRegistry>();
        self.world_mut()
            .resource_mut::<UiProjectorRegistry>()
            .register_projector(projector);
        self
    }
}
