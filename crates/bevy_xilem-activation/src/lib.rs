#![forbid(unsafe_code)]

use std::{
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ListenerOptions, Name, Stream, prelude::*,
};
use single_instance::SingleInstance;
use sysuri::UriScheme;

pub type Result<T> = std::result::Result<T, ActivationError>;

#[derive(Debug)]
pub enum ActivationError {
    InvalidConfig(String),
    Io(io::Error),
    Protocol(sysuri::Error),
    SingleInstance(String),
}

impl std::fmt::Display for ActivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(reason) => write!(f, "invalid activation config: {reason}"),
            Self::Io(error) => write!(f, "activation io error: {error}"),
            Self::Protocol(error) => write!(f, "protocol registration error: {error}"),
            Self::SingleInstance(error) => write!(f, "single-instance error: {error}"),
        }
    }
}

impl std::error::Error for ActivationError {}

impl From<io::Error> for ActivationError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<sysuri::Error> for ActivationError {
    fn from(value: sysuri::Error) -> Self {
        Self::Protocol(value)
    }
}

#[derive(Debug, Clone)]
pub struct ProtocolRegistration {
    pub scheme: String,
    pub description: String,
    pub executable: Option<PathBuf>,
    pub icon: Option<PathBuf>,
}

impl ProtocolRegistration {
    #[must_use]
    pub fn new(
        scheme: impl Into<String>,
        description: impl Into<String>,
        executable: Option<PathBuf>,
    ) -> Self {
        Self {
            scheme: scheme.into(),
            description: description.into(),
            executable,
            icon: None,
        }
    }

    #[must_use]
    pub fn with_icon(mut self, icon: PathBuf) -> Self {
        self.icon = Some(icon);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ActivationConfig {
    pub app_id: String,
    pub protocol: Option<ProtocolRegistration>,
}

impl ActivationConfig {
    #[must_use]
    pub fn new(app_id: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            protocol: None,
        }
    }

    #[must_use]
    pub fn with_protocol(mut self, protocol: ProtocolRegistration) -> Self {
        self.protocol = Some(protocol);
        self
    }
}

pub enum BootstrapOutcome {
    Primary(ActivationService),
    SecondaryForwarded,
}

pub struct ActivationService {
    startup_uris: Vec<String>,
    receiver: Receiver<String>,
    _single_instance: SingleInstance,
}

impl ActivationService {
    #[must_use]
    pub fn take_startup_uris(&mut self) -> Vec<String> {
        std::mem::take(&mut self.startup_uris)
    }

    #[must_use]
    pub fn drain_uris(&mut self) -> Vec<String> {
        let mut uris = Vec::new();
        while let Ok(uri) = self.receiver.try_recv() {
            uris.push(uri);
        }
        uris
    }
}

pub fn bootstrap(config: ActivationConfig) -> Result<BootstrapOutcome> {
    validate_config(&config)?;

    if let Some(protocol) = config.protocol.as_ref() {
        ensure_protocol_registered(protocol)?;
    }

    let startup_uris = collect_activation_uris(std::env::args().skip(1));
    let single_instance = SingleInstance::new(single_instance_name(&config.app_id).as_str())
        .map_err(|error| ActivationError::SingleInstance(error.to_string()))?;

    let name = ipc_name_for_app(&config.app_id)?;

    if single_instance.is_single() {
        let receiver = spawn_ipc_listener(name, listener_thread_name(&config.app_id))?;
        Ok(BootstrapOutcome::Primary(ActivationService {
            startup_uris,
            receiver,
            _single_instance: single_instance,
        }))
    } else {
        forward_uris_to_primary(&name, &startup_uris)?;
        Ok(BootstrapOutcome::SecondaryForwarded)
    }
}

pub fn ensure_protocol_registered(protocol: &ProtocolRegistration) -> Result<()> {
    let executable = match protocol.executable.clone() {
        Some(path) => path,
        None => std::env::current_exe()?,
    };

    let mut scheme = UriScheme::new(
        protocol.scheme.clone(),
        protocol.description.clone(),
        executable,
    );

    if let Some(icon) = protocol.icon.clone() {
        scheme = scheme.with_icon(icon);
    }

    if !scheme.is_valid_scheme() {
        return Err(ActivationError::InvalidConfig(format!(
            "scheme `{}` is invalid",
            scheme.scheme
        )));
    }

    sysuri::register(&scheme)?;
    Ok(())
}

fn spawn_ipc_listener(name: Name<'static>, thread_name: String) -> Result<Receiver<String>> {
    let listener = ListenerOptions::new().name(name).create_sync()?;
    let (sender, receiver) = mpsc::channel::<String>();

    thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else {
                    continue;
                };
                let mut reader = BufReader::new(stream);
                let mut line = String::new();

                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let payload = line.trim();
                            if !payload.is_empty() {
                                let _ = sender.send(payload.to_string());
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        })
        .map_err(ActivationError::Io)?;

    Ok(receiver)
}

fn forward_uris_to_primary(name: &Name<'_>, uris: &[String]) -> Result<()> {
    if uris.is_empty() {
        return Ok(());
    }

    let mut last_error: Option<io::Error> = None;

    for _ in 0..16 {
        match Stream::connect(name.borrow()) {
            Ok(mut stream) => {
                for uri in uris {
                    stream.write_all(uri.as_bytes())?;
                    stream.write_all(b"\n")?;
                }
                stream.flush()?;
                return Ok(());
            }
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(50));
            }
        }
    }

    Err(ActivationError::Io(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "failed to connect to primary activation listener",
        )
    })))
}

fn validate_config(config: &ActivationConfig) -> Result<()> {
    if config.app_id.trim().is_empty() {
        return Err(ActivationError::InvalidConfig(
            "app_id cannot be empty".to_string(),
        ));
    }
    Ok(())
}

fn collect_activation_uris<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .map(|value| value.as_ref().trim().to_string())
        .filter(|value| value.contains("://"))
        .collect()
}

fn normalize_app_id(app_id: &str) -> String {
    app_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
}

fn single_instance_name(app_id: &str) -> String {
    let normalized = normalize_app_id(app_id);

    #[cfg(target_os = "macos")]
    {
        return std::env::temp_dir()
            .join(format!("{normalized}.lock"))
            .to_string_lossy()
            .into_owned();
    }

    #[allow(unreachable_code)]
    normalized
}

fn listener_thread_name(app_id: &str) -> String {
    format!("{}-activation-listener", normalize_app_id(app_id))
}

fn ipc_name_for_app(app_id: &str) -> io::Result<Name<'static>> {
    let normalized = normalize_app_id(app_id);
    let token = format!("{normalized}.activation");

    if GenericNamespaced::is_supported() {
        token
            .to_ns_name::<GenericNamespaced>()
            .map(|name| name.into_owned())
    } else {
        let socket_path = std::env::temp_dir().join(format!("{token}.sock"));
        socket_path
            .to_string_lossy()
            .to_string()
            .to_fs_name::<GenericFilePath>()
            .map(|name| name.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_only_uri_like_arguments() {
        let args = vec![
            "--flag".to_string(),
            "pixiv://account/login?code=abc&via=login".to_string(),
            "https://example.com".to_string(),
            "plain-text".to_string(),
        ];

        let uris = collect_activation_uris(args);
        assert_eq!(uris.len(), 2);
        assert!(uris[0].starts_with("pixiv://"));
        assert!(uris[1].starts_with("https://"));
    }

    #[test]
    fn app_id_normalization_is_stable() {
        assert_eq!(
            normalize_app_id("Pixiv Client@Desktop"),
            "pixiv-client-desktop"
        );
    }

    #[test]
    fn empty_app_id_is_rejected() {
        let result = validate_config(&ActivationConfig::new("  "));
        assert!(result.is_err());
    }

    #[test]
    fn protocol_builder_keeps_scheme() {
        let registration = ProtocolRegistration::new("pixiv", "Pixiv", None);
        assert_eq!(registration.scheme, "pixiv");
        assert_eq!(registration.description, "Pixiv");
    }
}
