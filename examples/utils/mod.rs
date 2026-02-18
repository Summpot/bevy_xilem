use std::sync::OnceLock;

use tracing_subscriber::{EnvFilter, fmt};

const DEFAULT_LOG_FILTER: &str = "info,wgpu_core=warn,wgpu_hal=warn,bevy_render=warn,bevy_app=warn,masonry::widget=info,xilem_core=info,xilem_masonry=info,xilem_masonry::masonry_root=info,bevy_xilem=debug,calculator=debug,timer=debug,todo_list=debug,download_with_progress=debug,game_2048=debug,i18n_showcase=debug";

static LOGGING_INITIALIZED: OnceLock<()> = OnceLock::new();

/// Initialize process-wide tracing for examples.
///
/// If `RUST_LOG` is set it takes precedence over [`DEFAULT_LOG_FILTER`].
pub fn init_logging() {
    LOGGING_INITIALIZED.get_or_init(|| {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));

        let _ = fmt().with_env_filter(env_filter).try_init();
    });
}
