use bevy_app::App;
use bevy_asset::AssetServer;
use bevy_ecs::prelude::Component;
use fluent::{FluentResource, concurrent::FluentBundle};
use masonry::peniko::Blob;
use std::{fs, io, path::Path, sync::Arc};
use unic_langid::LanguageIdentifier;

use crate::{
    ActiveStyleSheetAsset, AppI18n, MasonryRuntime, ProjectionCtx, StyleSheet, StyleTypeRegistry,
    UiEventQueue, UiProjector, UiProjectorRegistry, UiView, XilemFontBridge,
    set_active_stylesheet_asset_path,
};

/// Synchronous source for binary assets (fonts).
pub enum SyncAssetSource<'a> {
    Bytes(&'a [u8]),
    FilePath(&'a str),
}

/// Synchronous source for textual assets (FTL bundles).
pub enum SyncTextSource<'a> {
    String(&'a str),
    FilePath(&'a str),
}

fn flush_pending_font_registrations(app: &mut App) {
    let pending = app
        .world_mut()
        .resource_mut::<XilemFontBridge>()
        .take_pending_fonts();

    if pending.is_empty() {
        return;
    }

    {
        let world = app.world_mut();
        world.init_resource::<UiEventQueue>();
        world.init_non_send_resource::<MasonryRuntime>();
    }

    let mut runtime = app.world_mut().non_send_resource_mut::<MasonryRuntime>();
    for font_bytes in pending {
        runtime
            .render_root
            .register_fonts(Blob::new(Arc::new(font_bytes)));
    }
}

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

    /// Load a RON stylesheet asset and bind it as the active runtime style source.
    ///
    /// The file is hot-reloaded through Bevy's asset pipeline.
    fn load_style_sheet(&mut self, asset_path: impl Into<String>) -> &mut Self;

    /// Register a selector type alias usable by `Selector::Type("...")` in stylesheet RON.
    fn register_style_selector_type<T: Component>(
        &mut self,
        selector_name: impl Into<String>,
    ) -> &mut Self;

    /// Register a font synchronously from bytes or filesystem path.
    ///
    /// Font registration is fail-fast and writes into the active Masonry/Xilem runtime font
    /// database used for text shaping.
    fn register_xilem_font(&mut self, source: SyncAssetSource<'_>) -> &mut Self;

    /// Register a Fluent bundle synchronously from in-memory text or filesystem path.
    ///
    /// Initializes [`AppI18n`] automatically when missing.
    fn register_i18n_bundle(
        &mut self,
        locale: &str,
        source: SyncTextSource<'_>,
        font_stack: Vec<&str>,
    ) -> &mut Self;

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

    fn load_style_sheet(&mut self, asset_path: impl Into<String>) -> &mut Self {
        let asset_path = asset_path.into();
        set_active_stylesheet_asset_path(self.world_mut(), asset_path);

        if let Some(path) = self
            .world()
            .get_resource::<ActiveStyleSheetAsset>()
            .and_then(|active| active.path.clone())
            && let Some(asset_server) = self.world().get_resource::<AssetServer>()
        {
            let handle = asset_server.load::<StyleSheet>(path);
            self.world_mut()
                .resource_mut::<ActiveStyleSheetAsset>()
                .handle = Some(handle);
        }

        self
    }

    fn register_style_selector_type<T: Component>(
        &mut self,
        selector_name: impl Into<String>,
    ) -> &mut Self {
        self.init_resource::<StyleTypeRegistry>();
        let mut registry = self.world_mut().resource_mut::<StyleTypeRegistry>();
        registry.register_type_aliases::<T>();
        registry.register_type_name::<T>(selector_name);
        self
    }

    fn register_xilem_font(&mut self, source: SyncAssetSource<'_>) -> &mut Self {
        let bytes = match source {
            SyncAssetSource::Bytes(data) => data.to_vec(),
            SyncAssetSource::FilePath(path) => fs::read(path)
                .unwrap_or_else(|error| panic!("failed to read font file `{path}`: {error}")),
        };

        self.init_resource::<XilemFontBridge>();
        let queued = self
            .world_mut()
            .resource_mut::<XilemFontBridge>()
            .register_font_bytes(&bytes);

        if queued {
            flush_pending_font_registrations(self);
        }

        self
    }

    fn register_i18n_bundle(
        &mut self,
        locale: &str,
        source: SyncTextSource<'_>,
        font_stack: Vec<&str>,
    ) -> &mut Self {
        let locale_id: LanguageIdentifier = locale
            .parse()
            .unwrap_or_else(|_| panic!("locale `{locale}` should parse"));
        let font_stack = font_stack.into_iter().map(String::from).collect::<Vec<_>>();

        let ftl_text = match source {
            SyncTextSource::String(text) => text.to_string(),
            SyncTextSource::FilePath(path) => fs::read_to_string(path).unwrap_or_else(|error| {
                panic!("failed to read localization file `{path}`: {error}")
            }),
        };

        let resource = FluentResource::try_new(ftl_text).unwrap_or_else(|(_, errors)| {
            panic!("failed to parse Fluent resource for locale `{locale}`: {errors:?}")
        });

        let mut bundle = FluentBundle::new_concurrent(vec![locale_id.clone()]);
        if let Err(errors) = bundle.add_resource(resource) {
            panic!("failed to add Fluent resource for locale `{locale}`: {errors:?}");
        }

        if self.world().get_resource::<AppI18n>().is_none() {
            self.insert_resource(AppI18n::new(locale_id.clone()));
        }

        let mut i18n = self.world_mut().resource_mut::<AppI18n>();
        if i18n.bundles.is_empty() {
            i18n.set_active_locale(locale_id.clone());
        }
        i18n.insert_bundle(locale_id, bundle, font_stack);

        self
    }

    fn register_xilem_font_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.register_xilem_font(SyncAssetSource::Bytes(bytes))
    }

    fn register_xilem_font_path(&mut self, path: impl AsRef<Path>) -> io::Result<&mut Self> {
        let path = path.as_ref();
        let path = path.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("font path `{}` is not valid UTF-8", path.display()),
            )
        })?;

        self.register_xilem_font(SyncAssetSource::FilePath(path));
        Ok(self)
    }
}
