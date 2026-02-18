use bevy_a11y::AccessibilityPlugin;
use bevy_app::App;
use bevy_input::InputPlugin;
use bevy_window::{PrimaryWindow, Window, WindowPlugin};
use xilem::winit::{dpi::Size, error::EventLoopError};

/// Compatibility window options applied to Bevy's primary window before `App::run()`.
#[derive(Clone, Debug, Default)]
pub struct BevyWindowOptions {
    resizable: Option<bool>,
    initial_inner_size: Option<Size>,
    min_inner_size: Option<Size>,
}

impl BevyWindowOptions {
    /// Sets whether the window is resizable.
    #[must_use]
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = Some(resizable);
        self
    }

    /// Sets the initial inner size.
    #[must_use]
    pub fn with_initial_inner_size<S: Into<Size>>(mut self, size: S) -> Self {
        self.initial_inner_size = Some(size.into());
        self
    }

    /// Sets the minimum inner size.
    #[must_use]
    pub fn with_min_inner_size<S: Into<Size>>(mut self, size: S) -> Self {
        self.min_inner_size = Some(size.into());
        self
    }
}

fn size_to_logical(size: Size) -> (f32, f32) {
    match size {
        Size::Physical(physical) => (physical.width as f32, physical.height as f32),
        Size::Logical(logical) => (logical.width as f32, logical.height as f32),
    }
}

fn apply_window_options(window: &mut Window, title: &str, options: &BevyWindowOptions) {
    window.title = title.to_string();

    if let Some(resizable) = options.resizable {
        window.resizable = resizable;
    }

    if let Some(initial_inner_size) = options.initial_inner_size {
        let (width, height) = size_to_logical(initial_inner_size);
        window.resolution.set(width, height);
    }

    if let Some(min_inner_size) = options.min_inner_size {
        let (min_width, min_height) = size_to_logical(min_inner_size);
        window.resize_constraints.min_width = min_width.max(1.0);
        window.resize_constraints.min_height = min_height.max(1.0);
    }
}

fn build_primary_window(title: &str, options: &BevyWindowOptions) -> Window {
    let mut window = Window::default();
    apply_window_options(&mut window, title, options);
    window
}

fn primary_window_exists(app: &mut App) -> bool {
    let mut query = app
        .world_mut()
        .query_filtered::<&Window, bevy_ecs::query::With<PrimaryWindow>>();
    query.iter(app.world_mut()).next().is_some()
}

fn ensure_native_windowing_plugins(app: &mut App, primary_window: &Window) {
    let had_primary_window = primary_window_exists(app);

    // Bevy's native window lifecycle depends on the same core plugin stack used
    // by `bevy::DefaultPlugins` for windowed apps.
    if !app.is_plugin_added::<AccessibilityPlugin>() {
        app.add_plugins(AccessibilityPlugin);
    }

    if !app.is_plugin_added::<InputPlugin>() {
        app.add_plugins(InputPlugin);
    }

    if !app.is_plugin_added::<WindowPlugin>() {
        app.add_plugins(WindowPlugin {
            primary_window: if had_primary_window {
                None
            } else {
                Some(primary_window.clone())
            },
            ..Default::default()
        });
    }

    if !app.is_plugin_added::<bevy_winit::WinitPlugin>() {
        app.add_plugins(bevy_winit::WinitPlugin::default());
    }
}

fn configure_primary_window(app: &mut App, title: &str, options: &BevyWindowOptions) {
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Window, bevy_ecs::query::With<PrimaryWindow>>();

    if let Some(mut window) = query.iter_mut(app.world_mut()).next() {
        apply_window_options(&mut window, title, options);
        return;
    }

    let window = build_primary_window(title, options);
    app.world_mut().spawn((window, PrimaryWindow));
}

/// Run a Bevy app using Bevy's native runner and default `bevy_winit` event loop.
///
/// This no longer creates a separate Xilem runner/event loop.
pub fn run_app(bevy_app: App, window_title: impl Into<String>) -> Result<(), EventLoopError> {
    run_app_with_window_options(bevy_app, window_title, |options| options)
}

/// Same as [`run_app`] with primary-window option overrides.
///
/// The closure receives and returns [`BevyWindowOptions`], preserving ergonomic
/// call sites while delegating execution to Bevy's own runner.
pub fn run_app_with_window_options(
    mut bevy_app: App,
    window_title: impl Into<String>,
    configure_window: impl Fn(BevyWindowOptions) -> BevyWindowOptions + Send + Sync + 'static,
) -> Result<(), EventLoopError> {
    let title = window_title.into();
    let options = configure_window(BevyWindowOptions::default());
    let primary_window = build_primary_window(&title, &options);
    ensure_native_windowing_plugins(&mut bevy_app, &primary_window);
    configure_primary_window(&mut bevy_app, &title, &options);

    let _ = bevy_app.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use xilem::winit::dpi::{LogicalSize, PhysicalSize};

    #[test]
    fn options_apply_initial_and_min_sizes() {
        let mut window = Window::default();
        let options = BevyWindowOptions::default()
            .with_initial_inner_size(LogicalSize::new(640.0, 480.0))
            .with_min_inner_size(PhysicalSize::new(320, 200))
            .with_resizable(false);

        apply_window_options(&mut window, "Test", &options);

        assert_eq!(window.title, "Test");
        assert_eq!(window.width(), 640.0);
        assert_eq!(window.height(), 480.0);
        assert_eq!(window.resize_constraints.min_width, 320.0);
        assert_eq!(window.resize_constraints.min_height, 200.0);
        assert!(!window.resizable);
    }
}
