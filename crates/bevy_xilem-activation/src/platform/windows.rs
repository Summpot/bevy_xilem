#[cfg(windows)]
use crate::{ActivationError, ResolvedProtocolRegistration, Result};

#[cfg(windows)]
use winreg::RegKey;
#[cfg(windows)]
use winreg::enums::*;

#[cfg(windows)]
pub(crate) fn register(protocol: &ResolvedProtocolRegistration) -> Result<()> {
    let executable = protocol.executable.to_str().ok_or_else(|| {
        ActivationError::InvalidConfig(format!(
            "executable path contains invalid UTF-8: {:?}",
            protocol.executable
        ))
    })?;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = hkcu
        .open_subkey_with_flags("Software\\Classes", KEY_WRITE)
        .map_err(|error| {
            ActivationError::Platform(format!("failed to open Software\\Classes: {error}"))
        })?;

    let (scheme_key, _) = classes.create_subkey(&protocol.scheme).map_err(|error| {
        ActivationError::Platform(format!("failed to create scheme key: {error}"))
    })?;

    scheme_key
        .set_value("", &protocol.description)
        .map_err(|error| {
            ActivationError::Platform(format!("failed to set description: {error}"))
        })?;

    scheme_key.set_value("URL Protocol", &"").map_err(|error| {
        ActivationError::Platform(format!("failed to set URL Protocol: {error}"))
    })?;

    if let Some(icon_path) = &protocol.icon {
        let icon_str = icon_path.to_str().ok_or_else(|| {
            ActivationError::InvalidConfig(format!(
                "icon path contains invalid UTF-8: {:?}",
                icon_path
            ))
        })?;

        let (icon_key, _) = scheme_key.create_subkey("DefaultIcon").map_err(|error| {
            ActivationError::Platform(format!("failed to create DefaultIcon key: {error}"))
        })?;

        icon_key
            .set_value("", &icon_str)
            .map_err(|error| ActivationError::Platform(format!("failed to set icon: {error}")))?;
    }

    let (command_key, _) = scheme_key
        .create_subkey("shell\\open\\command")
        .map_err(|error| {
            ActivationError::Platform(format!("failed to create command key: {error}"))
        })?;

    let command = format!("\"{}\" \"%1\"", executable);
    command_key
        .set_value("", &command)
        .map_err(|error| ActivationError::Platform(format!("failed to set command: {error}")))?;

    Ok(())
}
