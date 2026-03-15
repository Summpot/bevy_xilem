use crate::{ActivationError, ResolvedProtocolRegistration, Result};

#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(windows)]
pub(crate) mod windows;

pub(crate) fn register(protocol: &ResolvedProtocolRegistration) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        return linux::register(protocol);
    }

    #[cfg(target_os = "macos")]
    {
        return macos::register(protocol);
    }

    #[cfg(windows)]
    {
        return windows::register(protocol);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = protocol;
        Err(ActivationError::Platform(
            "custom URL activation is unsupported on this platform".to_string(),
        ))
    }
}
