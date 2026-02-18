use std::sync::OnceLock;

use tracing_subscriber::{EnvFilter, fmt};

const DEFAULT_LOG_FILTER: &str = "info,wgpu_core=warn,wgpu_hal=warn,bevy_render=warn,bevy_app=warn";

static LOGGING_INITIALIZED: OnceLock<()> = OnceLock::new();

/// Initialize a process-wide tracing subscriber for desktop GUI examples/apps.
///
/// If `RUST_LOG` is set, it takes precedence. Otherwise a default filter tuned for
/// GUI use is applied to suppress noisy lower-level renderer output.
///
/// This function is idempotent and safe to call multiple times.
pub fn init_logging() {
    LOGGING_INITIALIZED.get_or_init(|| {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));

        let _ = fmt().with_env_filter(env_filter).try_init();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_filter_suppresses_noisy_targets() {
        assert!(DEFAULT_LOG_FILTER.contains("wgpu_core=warn"));
        assert!(DEFAULT_LOG_FILTER.contains("wgpu_hal=warn"));
        assert!(DEFAULT_LOG_FILTER.contains("bevy_render=warn"));
    }

    #[test]
    fn init_logging_can_be_called_multiple_times() {
        init_logging();
        init_logging();
    }
}
