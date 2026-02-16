use std::sync::Arc;

use bevy_app::App;
use xilem::{
    AppState, EventLoop, WindowId, WindowOptions, WindowView, Xilem, core::map_state, view::label,
    window, winit::error::EventLoopError,
};

use crate::synthesize::SynthesizedUiViews;

type WindowConfigurator =
    dyn Fn(WindowOptions<BevyXilemRuntime>) -> WindowOptions<BevyXilemRuntime> + Send + Sync;

/// Runtime state used by the windowed GUI bridge.
pub struct BevyXilemRuntime {
    bevy_app: App,
    running: bool,
    window_id: WindowId,
    window_title: String,
    configure_window: Arc<WindowConfigurator>,
}

impl AppState for BevyXilemRuntime {
    fn keep_running(&self) -> bool {
        self.running
    }
}

impl BevyXilemRuntime {
    #[must_use]
    pub fn new(
        bevy_app: App,
        window_title: impl Into<String>,
        configure_window: Arc<WindowConfigurator>,
    ) -> Self {
        Self {
            bevy_app,
            running: true,
            window_id: WindowId::next(),
            window_title: window_title.into(),
            configure_window,
        }
    }

    fn update_and_first_root_or_label(
        &mut self,
        fallback_text: impl Into<String>,
    ) -> crate::UiView {
        self.bevy_app.update();
        self.bevy_app
            .world()
            .get_resource::<SynthesizedUiViews>()
            .and_then(|views| views.roots.first().cloned())
            .unwrap_or_else(|| Arc::new(label(fallback_text.into())))
    }
}

fn app_logic(
    state: &mut BevyXilemRuntime,
) -> impl Iterator<Item = WindowView<BevyXilemRuntime>> + use<> {
    let root_view = state.update_and_first_root_or_label("No synthesized bevy_xilem root");
    let window_id = state.window_id;
    let window_title = state.window_title.clone();
    let configure_window = state.configure_window.clone();

    std::iter::once(
        window(
            window_id,
            window_title,
            map_state(root_view, |_state: &mut BevyXilemRuntime, _| ()),
        )
        .with_options(move |options| {
            (configure_window)(options).on_close(|state: &mut BevyXilemRuntime| {
                state.running = false;
            })
        }),
    )
}

/// Run a Bevy app inside a GUI window while preserving the Bevy-driven synthesis architecture.
pub fn run_app(bevy_app: App, window_title: impl Into<String>) -> Result<(), EventLoopError> {
    run_app_with_window_options(bevy_app, window_title, |options| options)
}

/// Same as [`run_app`] with custom window options.
pub fn run_app_with_window_options(
    bevy_app: App,
    window_title: impl Into<String>,
    configure_window: impl Fn(WindowOptions<BevyXilemRuntime>) -> WindowOptions<BevyXilemRuntime>
    + Send
    + Sync
    + 'static,
) -> Result<(), EventLoopError> {
    let runtime = BevyXilemRuntime::new(bevy_app, window_title, Arc::new(configure_window));
    let app = Xilem::new(runtime, app_logic);
    app.run_in(EventLoop::with_user_event())
}
