#[cfg(target_os = "linux")]
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(target_os = "linux")]
use crate::{ActivationError, ResolvedProtocolRegistration, Result};

#[cfg(target_os = "linux")]
pub(crate) fn register(protocol: &ResolvedProtocolRegistration) -> Result<()> {
    let desktop_file_path = create_desktop_file(protocol)?;
    update_desktop_database()?;
    set_default_handler(protocol.scheme.as_str(), &desktop_file_path)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn create_desktop_file(protocol: &ResolvedProtocolRegistration) -> Result<PathBuf> {
    let applications_dir = default_applications_dir()?;
    fs::create_dir_all(&applications_dir).map_err(|error| {
        ActivationError::Platform(format!(
            "failed to create applications directory {:?}: {error}",
            applications_dir
        ))
    })?;

    let desktop_filename = format!("{}-handler.desktop", protocol.scheme);
    let desktop_file_path = applications_dir.join(&desktop_filename);

    let executable_str = protocol.executable.to_str().ok_or_else(|| {
        ActivationError::InvalidConfig(format!(
            "executable path contains invalid UTF-8: {:?}",
            protocol.executable
        ))
    })?;
    let icon_str = protocol
        .icon
        .as_ref()
        .and_then(|path| path.to_str())
        .unwrap_or("");

    let desktop_content = format!(
        r#"[Desktop Entry]
Version=1.0
Type=Application
Name={name}
Comment={description}
Exec={executable} %u
Icon={icon}
Terminal=false
Categories=Network;
MimeType=x-scheme-handler/{scheme};
"#,
        name = protocol.scheme,
        description = protocol.description,
        executable = executable_str,
        icon = icon_str,
        scheme = protocol.scheme,
    );

    fs::write(&desktop_file_path, desktop_content).map_err(|error| {
        ActivationError::Platform(format!(
            "failed to write desktop file {:?}: {error}",
            desktop_file_path
        ))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(&desktop_file_path)
            .map_err(|error| {
                ActivationError::Platform(format!(
                    "failed to get desktop file metadata {:?}: {error}",
                    desktop_file_path
                ))
            })?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&desktop_file_path, perms).map_err(|error| {
            ActivationError::Platform(format!(
                "failed to set desktop file permissions {:?}: {error}",
                desktop_file_path
            ))
        })?;
    }

    Ok(desktop_file_path)
}

#[cfg(target_os = "linux")]
fn default_applications_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|error| {
        ActivationError::Platform(format!(
            "failed to get HOME for desktop registration: {error}"
        ))
    })?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("applications"))
}

#[cfg(target_os = "linux")]
fn update_desktop_database() -> Result<()> {
    let applications_dir = default_applications_dir()?;
    let _ = Command::new("update-desktop-database")
        .arg(&applications_dir)
        .output();
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_default_handler(scheme: &str, desktop_file_path: &Path) -> Result<()> {
    let desktop_filename = desktop_file_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ActivationError::InvalidConfig(format!(
                "invalid desktop file path: {:?}",
                desktop_file_path
            ))
        })?;

    let mime_type = format!("x-scheme-handler/{scheme}");
    let output = Command::new("xdg-mime")
        .arg("default")
        .arg(desktop_filename)
        .arg(&mime_type)
        .output()
        .map_err(|error| ActivationError::Platform(format!("failed to run xdg-mime: {error}")))?;

    if !output.status.success() {
        return Err(ActivationError::Platform(format!(
            "xdg-mime failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(())
}
