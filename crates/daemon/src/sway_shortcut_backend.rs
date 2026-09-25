use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::shortcut_backend::{
    NamedKey, Shortcut, ShortcutActivation, ShortcutBackend, ShortcutBackendCapability,
    ShortcutError, ShortcutKey, ShortcutModifiers, ShortcutRegistrationOutcome,
};

const MAGIC: &[u8; 6] = b"i3-ipc";
const GET_CONFIG: u32 = 9;
const SWAY_IPC_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwayBindingDiagnosis {
    MatchedPookie,
    Conflict { command: String },
    NotFound,
}

#[derive(Debug, Clone)]
enum FallbackSource {
    HostFilesystem,
    Explicit(Option<String>),
}

pub struct SwayShortcutBackend {
    socket_path: Option<PathBuf>,
    fallback_source: FallbackSource,
    registered_shortcut: Option<Shortcut>,
}

impl SwayShortcutBackend {
    /// Attempts to discover Sway IPC socket via `$SWAYSOCK`.
    pub fn new() -> Result<Self, ShortcutError> {
        let socket_path = std::env::var_os("SWAYSOCK")
            .map(PathBuf::from)
            .filter(|p| p.exists());

        Ok(Self {
            socket_path,
            fallback_source: FallbackSource::HostFilesystem,
            registered_shortcut: None,
        })
    }

    /// Creates a backend with an explicit socket path and default host filesystem fallback.
    pub fn from_socket(socket_path: Option<PathBuf>) -> Self {
        Self {
            socket_path,
            fallback_source: FallbackSource::HostFilesystem,
            registered_shortcut: None,
        }
    }

    /// Creates an isolated offline backend with explicit fallback configuration content (or None).
    ///
    /// This constructor bypasses host filesystem inspection (`~/.config/sway/config`, `/etc/sway/config`)
    /// and IPC, providing 100% deterministic isolation for tests and controlled environments.
    pub fn from_offline_config(config: Option<&str>) -> Self {
        Self {
            socket_path: None,
            fallback_source: FallbackSource::Explicit(config.map(str::to_string)),
            registered_shortcut: None,
        }
    }

    /// Sets or overrides the fallback config source (for deterministic testing with or without IPC).
    pub fn with_fallback_config(mut self, config: Option<&str>) -> Self {
        self.fallback_source = FallbackSource::Explicit(config.map(str::to_string));
        self
    }

    pub fn socket_path(&self) -> Option<&Path> {
        self.socket_path.as_deref()
    }

    pub fn registered_shortcut(&self) -> Option<Shortcut> {
        self.registered_shortcut
    }
}

impl ShortcutBackend for SwayShortcutBackend {
    fn name(&self) -> &'static str {
        "Sway compositor-managed shortcut"
    }

    fn capability(&self) -> ShortcutBackendCapability {
        ShortcutBackendCapability::CompositorManaged
    }

    fn register(
        &mut self,
        shortcut: Shortcut,
    ) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        let binding_snippet = format_sway_binding(shortcut);

        // 1. Try authoritative Sway IPC query if socket is available
        if let Some(ref sock_path) = self.socket_path {
            match query_sway_config(sock_path) {
                Ok(active_config) => {
                    self.registered_shortcut = Some(shortcut);
                    let diagnosis = diagnose_sway_config(&active_config, shortcut);

                    return match diagnosis {
                        SwayBindingDiagnosis::MatchedPookie => {
                            Ok(ShortcutRegistrationOutcome::CompositorManaged {
                                binding_snippet,
                                verified: true,
                                conflict: None,
                                diagnostic: None,
                            })
                        }
                        SwayBindingDiagnosis::Conflict { command } => {
                            Ok(ShortcutRegistrationOutcome::CompositorManaged {
                                binding_snippet,
                                verified: false,
                                conflict: Some(format!(
                                    "Key is bound to '{command}' in active Sway configuration"
                                )),
                                diagnostic: None,
                            })
                        }
                        SwayBindingDiagnosis::NotFound => {
                            Ok(ShortcutRegistrationOutcome::CompositorManaged {
                                binding_snippet,
                                verified: false,
                                conflict: None,
                                diagnostic: None,
                            })
                        }
                    };
                }
                Err(err) => {
                    tracing::debug!(
                        error = ?err,
                        "Sway IPC GET_CONFIG query failed; trying diagnostic file fallback"
                    );
                }
            }
        }

        // 2. Diagnostic fallback to configuration file or explicit mock content (IPC unavailable)
        self.registered_shortcut = Some(shortcut);
        let fallback_content = match &self.fallback_source {
            FallbackSource::HostFilesystem => read_sway_config_file(),
            FallbackSource::Explicit(content) => content.clone(),
        };

        if let Some(file_content) = fallback_content {
            let diagnosis = diagnose_sway_config(&file_content, shortcut);
            match diagnosis {
                SwayBindingDiagnosis::MatchedPookie => {
                    // Must NOT report verified = true when IPC was unavailable
                    Ok(ShortcutRegistrationOutcome::CompositorManaged {
                        binding_snippet,
                        verified: false,
                        conflict: None,
                        diagnostic: None,
                    })
                }
                SwayBindingDiagnosis::Conflict { command } => {
                    Ok(ShortcutRegistrationOutcome::CompositorManaged {
                        binding_snippet,
                        verified: false,
                        conflict: Some(format!(
                            "Key is bound to '{command}' in configuration file (Sway IPC unavailable)"
                        )),
                        diagnostic: None,
                    })
                }
                SwayBindingDiagnosis::NotFound => {
                    Ok(ShortcutRegistrationOutcome::CompositorManaged {
                        binding_snippet,
                        verified: false,
                        conflict: None,
                        diagnostic: None,
                    })
                }
            }
        } else {
            Ok(ShortcutRegistrationOutcome::CompositorManaged {
                binding_snippet,
                verified: false,
                conflict: None,
                diagnostic: None,
            })
        }
    }

    fn wait_for_activation(&mut self) -> Result<ShortcutActivation, ShortcutError> {
        // In CompositorManaged mode, activation is external via daemon IPC (pookie-paste --toggle)
        Err(ShortcutError::Unavailable)
    }

    fn unregister(&mut self) -> Result<(), ShortcutError> {
        self.registered_shortcut = None;
        Ok(())
    }
}

/// Formats the canonical Sway `bindsym` directive for a given shortcut.
pub fn format_sway_binding(shortcut: Shortcut) -> String {
    let mut parts = Vec::new();

    if shortcut.modifiers.super_key {
        parts.push("Mod4");
    }
    if shortcut.modifiers.control {
        parts.push("Ctrl");
    }
    if shortcut.modifiers.alt {
        parts.push("Mod1");
    }
    if shortcut.modifiers.shift {
        parts.push("Shift");
    }

    let key_str = match shortcut.key {
        ShortcutKey::Character(c) => c.to_ascii_lowercase().to_string(),
        ShortcutKey::Named(named) => match named {
            NamedKey::Space => "space".to_string(),
            NamedKey::Tab => "Tab".to_string(),
            NamedKey::Enter => "Return".to_string(),
            NamedKey::Escape => "Escape".to_string(),
            NamedKey::Insert => "Insert".to_string(),
            NamedKey::Delete => "Delete".to_string(),
            NamedKey::F(n) => format!("F{n}"),
        },
    };

    parts.push(&key_str);
    let combo = parts.join("+");

    format!("bindsym {combo} exec pookie-paste --toggle")
}

/// Connects to the Sway IPC socket and issues a `GET_CONFIG` (type 9) request.
pub fn query_sway_config(socket_path: &Path) -> Result<String, ShortcutError> {
    let mut stream = UnixStream::connect(socket_path).map_err(|err| {
        ShortcutError::Failed(format!(
            "failed to connect to Sway IPC at {}: {err}",
            socket_path.display()
        ))
    })?;

    stream
        .set_read_timeout(Some(SWAY_IPC_TIMEOUT))
        .map_err(|err| {
            ShortcutError::Failed(format!("failed setting Sway IPC read timeout: {err}"))
        })?;
    stream
        .set_write_timeout(Some(SWAY_IPC_TIMEOUT))
        .map_err(|err| {
            ShortcutError::Failed(format!("failed setting Sway IPC write timeout: {err}"))
        })?;

    // Construct i3-ipc packet: MAGIC (6B) + Length (4B LE) + Type (4B LE) + Payload (empty)
    let mut request_header = Vec::with_capacity(14);
    request_header.extend_from_slice(MAGIC);
    request_header.extend_from_slice(&0u32.to_le_bytes());
    request_header.extend_from_slice(&GET_CONFIG.to_le_bytes());

    stream
        .write_all(&request_header)
        .map_err(|err| ShortcutError::Failed(format!("failed sending Sway GET_CONFIG: {err}")))?;
    stream
        .flush()
        .map_err(|err| ShortcutError::Failed(format!("failed flushing Sway IPC stream: {err}")))?;

    let mut response_header = [0u8; 14];
    stream.read_exact(&mut response_header).map_err(|err| {
        ShortcutError::Failed(format!("failed reading Sway response header: {err}"))
    })?;

    if &response_header[0..6] != MAGIC {
        return Err(ShortcutError::Failed(
            "invalid Sway IPC response magic".to_string(),
        ));
    }

    let payload_len = u32::from_le_bytes(
        response_header[6..10]
            .try_into()
            .expect("slice with exact size 4"),
    ) as usize;

    let mut payload_buf = vec![0u8; payload_len];
    stream.read_exact(&mut payload_buf).map_err(|err| {
        ShortcutError::Failed(format!("failed reading Sway response payload: {err}"))
    })?;

    let json: serde_json::Value = serde_json::from_slice(&payload_buf).map_err(|err| {
        ShortcutError::Failed(format!("failed parsing Sway response JSON: {err}"))
    })?;

    let config = json.get("config").and_then(|c| c.as_str()).ok_or_else(|| {
        ShortcutError::Failed("Sway GET_CONFIG missing 'config' field".to_string())
    })?;

    Ok(config.to_string())
}

/// Diagnoses whether a given `Shortcut` is configured in Sway configuration text.
pub fn diagnose_sway_config(config_text: &str, shortcut: Shortcut) -> SwayBindingDiagnosis {
    let mut variables: HashMap<String, String> = HashMap::new();

    // First pass: extract variable definitions like `set $mod Mod4`
    for raw_line in config_text.lines() {
        let line = strip_comments(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        let mut parts = line.split_whitespace();
        if let (Some("set"), Some(var_name), Some(var_value)) =
            (parts.next(), parts.next(), parts.next())
            && var_name.starts_with('$')
        {
            variables.insert(var_name.to_string(), var_value.to_string());
        }
    }

    // Second pass: scan `bindsym` lines
    for raw_line in config_text.lines() {
        let line = strip_comments(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        let mut tokens = line.split_whitespace();
        let Some(first) = tokens.next() else { continue };
        if first != "bindsym" {
            continue;
        }

        // Skip any leading flag arguments like `--to-code`, `--release`, `--locked`, `--no-warn`
        let mut key_combo_opt = None;
        for token in tokens.by_ref() {
            if token.starts_with("--") {
                continue;
            }
            key_combo_opt = Some(token);
            break;
        }

        let Some(key_combo) = key_combo_opt else {
            continue;
        };

        // The remaining tokens form the executed command
        let command_tokens: Vec<&str> = tokens.collect();
        let command = command_tokens.join(" ");

        if matches_key_combination(key_combo, &variables, shortcut) {
            if is_pookie_command(&command) {
                return SwayBindingDiagnosis::MatchedPookie;
            } else {
                return SwayBindingDiagnosis::Conflict { command };
            }
        }
    }

    SwayBindingDiagnosis::NotFound
}

/// Checks if a key combination string in Sway syntax matches the target `Shortcut`.
fn matches_key_combination(
    key_combo: &str,
    variables: &HashMap<String, String>,
    shortcut: Shortcut,
) -> bool {
    let mut parsed_modifiers = ShortcutModifiers::NONE;
    let mut parsed_key: Option<String> = None;

    for part in key_combo.split('+') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }

        // Resolve variable if applicable (e.g. $mod -> Mod4)
        let resolved_storage;
        let resolved = if token.starts_with('$') {
            if let Some(val) = variables.get(token) {
                resolved_storage = val.clone();
                &resolved_storage
            } else {
                token
            }
        } else {
            token
        };

        match resolved.to_ascii_lowercase().as_str() {
            "mod4" | "super" | "logo" | "win" => parsed_modifiers.super_key = true,
            "ctrl" | "control" => parsed_modifiers.control = true,
            "mod1" | "alt" => parsed_modifiers.alt = true,
            "shift" => parsed_modifiers.shift = true,
            other => {
                parsed_key = Some(other.to_string());
            }
        }
    }

    if parsed_modifiers != shortcut.modifiers {
        return false;
    }

    let Some(key_token) = parsed_key else {
        return false;
    };

    match shortcut.key {
        ShortcutKey::Character(c) => {
            key_token.eq_ignore_ascii_case(&c.to_ascii_lowercase().to_string())
        }
        ShortcutKey::Named(named) => match named {
            NamedKey::Space => key_token.eq_ignore_ascii_case("space"),
            NamedKey::Tab => key_token.eq_ignore_ascii_case("tab"),
            NamedKey::Enter => {
                key_token.eq_ignore_ascii_case("return") || key_token.eq_ignore_ascii_case("enter")
            }
            NamedKey::Escape => {
                key_token.eq_ignore_ascii_case("escape") || key_token.eq_ignore_ascii_case("esc")
            }
            NamedKey::Insert => {
                key_token.eq_ignore_ascii_case("insert") || key_token.eq_ignore_ascii_case("ins")
            }
            NamedKey::Delete => {
                key_token.eq_ignore_ascii_case("delete") || key_token.eq_ignore_ascii_case("del")
            }
            NamedKey::F(n) => key_token.eq_ignore_ascii_case(&format!("f{n}")),
        },
    }
}

/// Precisely identifies whether an executed Sway command targets Pookie Paste.
///
/// Recognizes `pookie-paste --toggle`, `pookie-paste -t`, `pookie-paste-ui`, and paths
/// while rejecting wrappers or unrelated commands that happen to contain substring matches.
pub fn is_pookie_command(command: &str) -> bool {
    let mut tokens = command.split_whitespace();

    // Strip optional leading 'exec' and flag '--no-startup-id'
    let mut binary_token_opt = tokens.next();
    if binary_token_opt == Some("exec") {
        binary_token_opt = tokens.next();
        if binary_token_opt == Some("--no-startup-id") {
            binary_token_opt = tokens.next();
        }
    }

    let Some(binary_token) = binary_token_opt else {
        return false;
    };

    let binary_name = Path::new(binary_token)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(binary_token);

    if binary_name == "pookie-paste" {
        tokens.any(|arg| arg == "--toggle" || arg == "-t")
    } else {
        binary_name == "pookie-paste-ui"
    }
}

fn strip_comments(line: &str) -> &str {
    line.split('#').next().unwrap_or("")
}

fn read_sway_config_file() -> Option<String> {
    let candidates = [
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(|c| PathBuf::from(c).join("sway/config")),
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".config/sway/config")),
        Some(PathBuf::from("/etc/sway/config")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if let Ok(content) = std::fs::read_to_string(&candidate) {
            return Some(content);
        }
    }

    None
}
