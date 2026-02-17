use bevy_app::App;
use bevy_ecs::prelude::Component;
use std::{io, path::Path};

use crate::{ProjectionCtx, UiProjector, UiProjectorRegistry, UiView, XilemFontBridge};

/// Fluent extension methods for registering bevy_xilem projectors on a Bevy [`App`].
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
///
/// use bevy_xilem::{
///     AppBevyXilemExt, BevyXilemPlugin, ProjectionCtx, UiRoot, UiView,
///     bevy_app::{App, Startup},
///     bevy_ecs::prelude::*,
///     text_button,
/// };
///
/// #[derive(Component, Clone, Copy)]
/// struct Root;
///
/// #[derive(Debug, Clone, Copy)]
/// enum Action {
///     Clicked,
/// }
///
/// fn project_root(_: &Root, ctx: ProjectionCtx<'_>) -> UiView {
///     Arc::new(text_button(ctx.entity, Action::Clicked, "Click"))
/// }
///
/// fn setup(mut commands: Commands) {
///     commands.spawn((UiRoot, Root));
/// }
///
/// let mut app = App::new();
/// app.add_plugins(BevyXilemPlugin)
///     .register_projector::<Root>(project_root)
///     .add_systems(Startup, setup);
/// ```
pub trait AppBevyXilemExt {
    /// Register a typed projector for a specific component.
    ///
    /// Last registered projector has precedence during projection.
    fn register_projector<C: Component>(
        &mut self,
        projector: fn(&C, ProjectionCtx<'_>) -> UiView,
    ) -> &mut Self;

    /// Register a raw projector implementation.
    ///
    /// Use this when component-based registration is insufficient.
    fn register_raw_projector<P: UiProjector>(&mut self, projector: P) -> &mut Self;

    /// Queue raw font bytes for registration in Xilem/Masonry text shaping.
    ///
    /// This bridges app-provided fonts into Xilem's font database.
    fn register_xilem_font_bytes(&mut self, bytes: &[u8]) -> &mut Self;

    /// Read and queue a font file for registration in Xilem/Masonry text shaping.
    ///
    /// Typical path for Bevy projects: `assets/fonts/<font-file>.ttf|otf`.
    fn register_xilem_font_path(&mut self, path: impl AsRef<Path>) -> io::Result<&mut Self>;
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

    fn register_xilem_font_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.init_resource::<XilemFontBridge>();
        self.world_mut()
            .resource_mut::<XilemFontBridge>()
            .register_font_bytes(bytes);
        self
    }

    fn register_xilem_font_path(&mut self, path: impl AsRef<Path>) -> io::Result<&mut Self> {
        self.init_resource::<XilemFontBridge>();
        self.world_mut()
            .resource_mut::<XilemFontBridge>()
            .register_font_path(path)?;
        Ok(self)
    }
}
