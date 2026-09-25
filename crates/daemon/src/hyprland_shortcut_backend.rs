use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::shortcut_backend::{
    NamedKey, Shortcut, ShortcutActivation, ShortcutBackend, ShortcutBackendCapability,
    ShortcutError, ShortcutKey, ShortcutModifiers, ShortcutRegistrationOutcome,
};
use crate::sway_shortcut_backend::is_pookie_command;

const HYPRLAND_IPC_TIMEOUT: Duration = Duration::from_millis(500);

pub const HYPR_MOD_SHIFT: u32 = 1;
pub const HYPR_MOD_CTRL: u32 = 4;
pub const HYPR_MOD_ALT: u32 = 8;
pub const HYPR_MOD_SUPER: u32 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HyprlandBindingDiagnosis {
    /// Actively bound in compositor and proven to invoke Pookie Paste (direct exec).
    VerifiedPookie,
    /// Actively bound to another command or non-Lua dispatcher.
    Conflict { command: String },
    /// Actively bound to a Lua callback (__lua), so runtime target is opaque.
    OccupiedOpaque { callback_id: String },
    /// No binding found for this shortcut.
    NotFound,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct HyprlandIpcBind {
    #[serde(default)]
    pub modmask: u32,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub dispatcher: String,
    #[serde(default)]
    pub arg: String,
    #[serde(default)]
    pub submap: String,
}

#[derive(Debug, Clone)]
enum FallbackSource {
    HostFilesystem,
    Explicit(Option<String>),
}

#[derive(Debug, Clone)]
enum IpcSource {
    LiveSocket(Option<PathBuf>),
    MockResponse(String),
}

pub struct HyprlandShortcutBackend {
    ipc_source: IpcSource,
    fallback_source: FallbackSource,
    registered_shortcut: Option<Shortcut>,
}

impl HyprlandShortcutBackend {
    /// Attempts to discover Hyprland IPC command socket via `$HYPRLAND_INSTANCE_SIGNATURE`.
    pub fn new() -> Result<Self, ShortcutError> {
        let socket_path = find_hyprland_socket();
        Ok(Self {
            ipc_source: IpcSource::LiveSocket(socket_path),
            fallback_source: FallbackSource::HostFilesystem,
            registered_shortcut: None,
        })
    }

    /// Creates a backend with an explicit socket path and default host filesystem fallback.
    pub fn from_socket(socket_path: Option<PathBuf>) -> Self {
        Self {
            ipc_source: IpcSource::LiveSocket(socket_path),
            fallback_source: FallbackSource::HostFilesystem,
            registered_shortcut: None,
        }
    }

    /// Creates an isolated offline backend with explicit fallback configuration content (or None).
    ///
    /// This constructor bypasses host filesystem inspection (`~/.config/hypr/hyprland.lua`, etc.)
    /// and IPC, providing 100% deterministic isolation for tests and controlled environments.
    pub fn from_offline_config(config: Option<&str>) -> Self {
        Self {
            ipc_source: IpcSource::LiveSocket(None),
            fallback_source: FallbackSource::Explicit(config.map(str::to_string)),
            registered_shortcut: None,
        }
    }

    /// Creates an isolated backend with a mock `j/binds` JSON response for pure deterministic testing.
    pub fn with_mock_ipc(raw_json: &str) -> Self {
        Self {
            ipc_source: IpcSource::MockResponse(raw_json.to_string()),
            fallback_source: FallbackSource::Explicit(None),
            registered_shortcut: None,
        }
    }

    /// Sets or overrides the fallback config source (for deterministic testing).
    pub fn with_fallback_config(mut self, config: Option<&str>) -> Self {
        self.fallback_source = FallbackSource::Explicit(config.map(str::to_string));
        self
    }

    pub fn socket_path(&self) -> Option<&Path> {
        match &self.ipc_source {
            IpcSource::LiveSocket(sock) => sock.as_deref(),
            IpcSource::MockResponse(_) => None,
        }
    }

    pub fn registered_shortcut(&self) -> Option<Shortcut> {
        self.registered_shortcut
    }
}

impl ShortcutBackend for HyprlandShortcutBackend {
    fn name(&self) -> &'static str {
        "Hyprland compositor-managed shortcut"
    }

    fn capability(&self) -> ShortcutBackendCapability {
        ShortcutBackendCapability::CompositorManaged
    }

    fn register(
        &mut self,
        shortcut: Shortcut,
    ) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        let binding_snippet = format_hyprland_binding(shortcut);
        self.registered_shortcut = Some(shortcut);

        // 1. Try authoritative Hyprland IPC query (live socket or mock response)
        let ipc_response_opt = match &self.ipc_source {
            IpcSource::LiveSocket(Some(sock_path)) => query_hyprland_binds(sock_path).ok(),
            IpcSource::MockResponse(raw_json) => Some(raw_json.clone()),
            _ => None,
        };

        if let Some(raw_binds_json) = ipc_response_opt {
            let diagnosis = diagnose_hyprland_ipc_binds(&raw_binds_json, shortcut);

            return match diagnosis {
                HyprlandBindingDiagnosis::VerifiedPookie => {
                    Ok(ShortcutRegistrationOutcome::CompositorManaged {
                        binding_snippet,
                        verified: true,
                        conflict: None,
                        diagnostic: Some(
                            "Verified active in running Hyprland compositor (direct exec)"
                                .to_string(),
                        ),
                    })
                }
                HyprlandBindingDiagnosis::Conflict { command } => {
                    Ok(ShortcutRegistrationOutcome::CompositorManaged {
                        binding_snippet,
                        verified: false,
                        conflict: Some(format!(
                            "Key is bound to '{command}' in active Hyprland configuration"
                        )),
                        diagnostic: Some("Conflicting binding in active compositor".to_string()),
                    })
                }
                HyprlandBindingDiagnosis::OccupiedOpaque { callback_id } => {
                    // Check if static config provides optional diagnostic context without overriding live IPC truth
                    let static_note = match &self.fallback_source {
                        FallbackSource::HostFilesystem => read_hyprland_config_file(),
                        FallbackSource::Explicit(content) => content.clone(),
                    };

                    let pookie_literal_found = static_note.as_deref().is_some_and(|content| {
                        diagnose_hyprland_config_text(content, shortcut)
                            == HyprlandBindingDiagnosis::VerifiedPookie
                    });

                    let diagnostic_msg = if pookie_literal_found {
                        format!(
                            "Key is actively bound to a Lua callback (__lua, id: {callback_id}) in Hyprland. Static config contains a literal Pookie binding for this shortcut, but runtime target cannot be verified over IPC"
                        )
                    } else {
                        format!(
                            "Key is actively bound to a Lua callback (__lua, id: {callback_id}) in Hyprland; runtime command target cannot be verified over IPC"
                        )
                    };

                    // Opaque Lua callback: occupancy is confirmed, but command identity is opaque.
                    // Must NOT claim verified=true, but also not a definite conflict.
                    Ok(ShortcutRegistrationOutcome::CompositorManaged {
                        binding_snippet,
                        verified: false,
                        conflict: None,
                        diagnostic: Some(diagnostic_msg),
                    })
                }
                HyprlandBindingDiagnosis::NotFound => {
                    Ok(ShortcutRegistrationOutcome::CompositorManaged {
                        binding_snippet,
                        verified: false,
                        conflict: None,
                        diagnostic: None,
                    })
                }
            };
        }

        // 2. Diagnostic fallback to configuration file on disk (IPC unavailable)
        let fallback_content = match &self.fallback_source {
            FallbackSource::HostFilesystem => read_hyprland_config_file(),
            FallbackSource::Explicit(content) => content.clone(),
        };

        if let Some(file_content) = fallback_content {
            let diagnosis = diagnose_hyprland_config_text(&file_content, shortcut);
            match diagnosis {
                HyprlandBindingDiagnosis::VerifiedPookie => {
                    // Must NOT report verified = true when IPC was unavailable
                    Ok(ShortcutRegistrationOutcome::CompositorManaged {
                        binding_snippet,
                        verified: false,
                        conflict: None,
                        diagnostic: Some(
                            "Found matching Pookie binding in static configuration file (Hyprland IPC unavailable)"
                                .to_string(),
                        ),
                    })
                }
                HyprlandBindingDiagnosis::Conflict { command } => {
                    Ok(ShortcutRegistrationOutcome::CompositorManaged {
                        binding_snippet,
                        verified: false,
                        conflict: Some(format!(
                            "Key is bound to '{command}' in configuration file (Hyprland IPC unavailable)"
                        )),
                        diagnostic: Some("Conflicting binding in static configuration".to_string()),
                    })
                }
                HyprlandBindingDiagnosis::OccupiedOpaque { .. }
                | HyprlandBindingDiagnosis::NotFound => {
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

/// Formats the modern Hyprland Lua binding directive (Hyprland 0.56+) for a given shortcut.
pub fn format_hyprland_lua_binding(shortcut: Shortcut) -> String {
    let mut parts: Vec<String> = Vec::new();

    if shortcut.modifiers.super_key {
        parts.push("SUPER".to_string());
    }
    if shortcut.modifiers.control {
        parts.push("CTRL".to_string());
    }
    if shortcut.modifiers.alt {
        parts.push("ALT".to_string());
    }
    if shortcut.modifiers.shift {
        parts.push("SHIFT".to_string());
    }

    let key_str = match shortcut.key {
        ShortcutKey::Character(c) => c.to_ascii_uppercase().to_string(),
        ShortcutKey::Named(named) => match named {
            NamedKey::Space => "Space".to_string(),
            NamedKey::Tab => "Tab".to_string(),
            NamedKey::Enter => "Return".to_string(),
            NamedKey::Escape => "Escape".to_string(),
            NamedKey::Insert => "Insert".to_string(),
            NamedKey::Delete => "Delete".to_string(),
            NamedKey::F(n) => format!("F{n}"),
        },
    };
    parts.push(key_str);

    let combo = parts.join(" + ");
    format!("hl.bind(\"{combo}\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))")
}

/// Formats the legacy / classic Hyprlang binding directive (`hyprland.conf`) for a given shortcut.
pub fn format_hyprland_hyprlang_binding(shortcut: Shortcut) -> String {
    let mut mod_parts = Vec::new();

    if shortcut.modifiers.super_key {
        mod_parts.push("SUPER");
    }
    if shortcut.modifiers.control {
        mod_parts.push("CTRL");
    }
    if shortcut.modifiers.alt {
        mod_parts.push("ALT");
    }
    if shortcut.modifiers.shift {
        mod_parts.push("SHIFT");
    }

    let mods_str = if mod_parts.is_empty() {
        "".to_string()
    } else {
        mod_parts.join(" ")
    };

    let key_str = match shortcut.key {
        ShortcutKey::Character(c) => c.to_ascii_uppercase().to_string(),
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

    if mods_str.is_empty() {
        format!("bind = , {key_str}, exec, pookie-paste --toggle")
    } else {
        format!("bind = {mods_str}, {key_str}, exec, pookie-paste --toggle")
    }
}

/// Formats the canonical Hyprland binding directive for a given shortcut.
///
/// Defaults to modern Lua syntax for Hyprland 0.56+. For legacy `hyprland.conf`,
/// see [`format_hyprland_hyprlang_binding`].
pub fn format_hyprland_binding(shortcut: Shortcut) -> String {
    format_hyprland_lua_binding(shortcut)
}

/// Calculates the expected Hyprland bitmask from `ShortcutModifiers`.
pub fn shortcut_to_hyprland_modmask(shortcut: Shortcut) -> u32 {
    let mut mask = 0u32;
    if shortcut.modifiers.shift {
        mask |= HYPR_MOD_SHIFT;
    }
    if shortcut.modifiers.control {
        mask |= HYPR_MOD_CTRL;
    }
    if shortcut.modifiers.alt {
        mask |= HYPR_MOD_ALT;
    }
    if shortcut.modifiers.super_key {
        mask |= HYPR_MOD_SUPER;
    }
    mask
}

/// Matches a key string from Hyprland against `ShortcutKey`.
pub fn matches_hyprland_key(key: &str, shortcut_key: ShortcutKey) -> bool {
    let key = key.trim();
    match shortcut_key {
        ShortcutKey::Character(c) => {
            key.len() == 1 && key.chars().next().unwrap().eq_ignore_ascii_case(&c)
        }
        ShortcutKey::Named(named) => match named {
            NamedKey::Space => key.eq_ignore_ascii_case("space"),
            NamedKey::Tab => key.eq_ignore_ascii_case("tab"),
            NamedKey::Enter => {
                key.eq_ignore_ascii_case("return") || key.eq_ignore_ascii_case("enter")
            }
            NamedKey::Escape => {
                key.eq_ignore_ascii_case("escape") || key.eq_ignore_ascii_case("esc")
            }
            NamedKey::Insert => {
                key.eq_ignore_ascii_case("insert") || key.eq_ignore_ascii_case("ins")
            }
            NamedKey::Delete => {
                key.eq_ignore_ascii_case("delete") || key.eq_ignore_ascii_case("del")
            }
            NamedKey::F(n) => key.eq_ignore_ascii_case(&format!("f{n}")),
        },
    }
}

/// Evaluates a raw `j/binds` JSON response against a requested `Shortcut`.
///
/// Uses a deterministic precedence rule across all matching default-submap entries:
/// 1. Definite Conflict takes top precedence.
/// 2. Verified Pookie (direct exec) takes second precedence.
/// 3. Occupied Opaque (__lua callback) takes third precedence.
/// 4. Not Configured if no entries match.
pub fn diagnose_hyprland_ipc_binds(raw_json: &str, shortcut: Shortcut) -> HyprlandBindingDiagnosis {
    let binds: Vec<HyprlandIpcBind> = match serde_json::from_str(raw_json.trim()) {
        Ok(b) => b,
        Err(_) => return HyprlandBindingDiagnosis::NotFound,
    };

    let expected_mask = shortcut_to_hyprland_modmask(shortcut);

    // Filter matching entries, prioritizing the default/global submap ("")
    let matching_default: Vec<&HyprlandIpcBind> = binds
        .iter()
        .filter(|b| {
            b.submap.is_empty()
                && b.modmask == expected_mask
                && matches_hyprland_key(&b.key, shortcut.key)
        })
        .collect();

    if matching_default.is_empty() {
        return HyprlandBindingDiagnosis::NotFound;
    }

    // Precedence rule evaluation:
    // 1. Check for any definite conflict
    for bind in &matching_default {
        if bind.dispatcher == "exec" {
            if !is_pookie_command(&bind.arg) {
                return HyprlandBindingDiagnosis::Conflict {
                    command: format!("exec {}", bind.arg),
                };
            }
        } else if bind.dispatcher != "__lua" {
            // Other compositor dispatcher (killactive, focus, etc.)
            let cmd = if bind.arg.is_empty() {
                bind.dispatcher.clone()
            } else {
                format!("{} {}", bind.dispatcher, bind.arg)
            };
            return HyprlandBindingDiagnosis::Conflict { command: cmd };
        }
    }

    // 2. Check for any verified Pookie exec binding
    for bind in &matching_default {
        if bind.dispatcher == "exec" && is_pookie_command(&bind.arg) {
            return HyprlandBindingDiagnosis::VerifiedPookie;
        }
    }

    // 3. Check for opaque Lua binding
    for bind in &matching_default {
        if bind.dispatcher == "__lua" {
            return HyprlandBindingDiagnosis::OccupiedOpaque {
                callback_id: bind.arg.clone(),
            };
        }
    }

    HyprlandBindingDiagnosis::NotFound
}

/// Connects to Hyprland's Unix socket and queries `j/binds`.
pub fn query_hyprland_binds(socket_path: &Path) -> Result<String, ShortcutError> {
    let mut stream = UnixStream::connect(socket_path).map_err(|err| {
        ShortcutError::Failed(format!(
            "failed connecting to Hyprland socket at {}: {err}",
            socket_path.display()
        ))
    })?;

    stream
        .set_read_timeout(Some(HYPRLAND_IPC_TIMEOUT))
        .map_err(|err| {
            ShortcutError::Failed(format!("failed setting Hyprland read timeout: {err}"))
        })?;
    stream
        .set_write_timeout(Some(HYPRLAND_IPC_TIMEOUT))
        .map_err(|err| {
            ShortcutError::Failed(format!("failed setting Hyprland write timeout: {err}"))
        })?;

    stream
        .write_all(b"j/binds")
        .map_err(|err| ShortcutError::Failed(format!("failed sending 'j/binds': {err}")))?;
    stream
        .flush()
        .map_err(|err| ShortcutError::Failed(format!("failed flushing stream: {err}")))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|err| ShortcutError::Failed(format!("failed reading Hyprland response: {err}")))?;

    Ok(response)
}

/// Diagnoses static configuration text (either `hyprland.lua` or `hyprland.conf`).
pub fn diagnose_hyprland_config_text(
    config_text: &str,
    shortcut: Shortcut,
) -> HyprlandBindingDiagnosis {
    // 1. Check for narrow literal Lua bindings:
    // e.g. hl.bind("SUPER + V", hl.dsp.exec_cmd("pookie-paste --toggle"))
    if let Some(diag) = scan_literal_lua_bindings(config_text, shortcut) {
        return diag;
    }

    // 2. Check for legacy/classic Hyprlang bindings:
    // e.g. bind = SUPER, V, exec, pookie-paste --toggle
    scan_hyprlang_bindings(config_text, shortcut)
}

fn scan_literal_lua_bindings(
    config_text: &str,
    shortcut: Shortcut,
) -> Option<HyprlandBindingDiagnosis> {
    let key_char_opt = match shortcut.key {
        ShortcutKey::Character(c) => Some(c.to_ascii_uppercase()),
        _ => None,
    };

    for raw_line in config_text.lines() {
        let line = strip_lua_comment(raw_line);
        if !line.contains("hl.bind") {
            continue;
        }

        // Check if line mentions our modifiers and key
        let has_super = line.contains("SUPER");
        let has_ctrl = line.contains("CTRL") || line.contains("CONTROL");
        let has_alt = line.contains("ALT");
        let has_shift = line.contains("SHIFT");

        if has_super != shortcut.modifiers.super_key
            || has_ctrl != shortcut.modifiers.control
            || has_alt != shortcut.modifiers.alt
            || has_shift != shortcut.modifiers.shift
        {
            continue;
        }

        let key_matches = match shortcut.key {
            ShortcutKey::Character(c) => {
                line.contains(&format!("\"{c}\""))
                    || line.contains(&format!("'{c}'"))
                    || line.contains(&format!("\"{}\"", c.to_ascii_uppercase()))
                    || line.contains(&format!("'{}'", c.to_ascii_uppercase()))
                    || line.contains(&format!("+ {c}"))
                    || line.contains(&format!("+ {}", c.to_ascii_uppercase()))
                    || line.contains(&format!(", {c}"))
                    || line.contains(&format!(", {}", c.to_ascii_uppercase()))
            }
            ShortcutKey::Named(NamedKey::Space) => line.contains("space") || line.contains("Space"),
            ShortcutKey::Named(NamedKey::F(n)) => {
                line.contains(&format!("F{n}")) || line.contains(&format!("f{n}"))
            }
            _ => false,
        };

        if !key_matches {
            continue;
        }

        if let Some(cmd) = extract_lua_exec_cmd(line) {
            if is_pookie_command(cmd) {
                return Some(HyprlandBindingDiagnosis::VerifiedPookie);
            } else {
                return Some(HyprlandBindingDiagnosis::Conflict {
                    command: format!("exec {cmd}"),
                });
            }
        } else if line.contains("hl.dsp.") {
            return Some(HyprlandBindingDiagnosis::Conflict {
                command: line.to_string(),
            });
        }
    }

    let _ = key_char_opt;
    None
}

fn extract_lua_exec_cmd(line: &str) -> Option<&str> {
    let idx = line.find("exec_cmd(")?;
    let after = &line[idx + "exec_cmd(".len()..];
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let inner = &after[1..];
    let end_quote = inner.find(quote)?;
    Some(&inner[..end_quote])
}

fn strip_lua_comment(line: &str) -> &str {
    let mut in_quote = None;
    let mut prev_char = '\0';
    let chars: Vec<(usize, char)> = line.char_indices().collect();

    for i in 0..chars.len() {
        let (idx, ch) = chars[i];
        if let Some(q) = in_quote {
            if ch == q && prev_char != '\\' {
                in_quote = None;
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
        } else if ch == '-' && i + 1 < chars.len() && chars[i + 1].1 == '-' {
            return line[..idx].trim();
        }
        prev_char = ch;
    }

    line.trim()
}

fn scan_hyprlang_bindings(config_text: &str, shortcut: Shortcut) -> HyprlandBindingDiagnosis {
    let mut variables: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // 1. Variable extraction ($mainMod = SUPER)
    for raw_line in config_text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if let Some((var, val)) = line.split_once('=') {
            let var = var.trim();
            if var.starts_with('$') {
                variables.insert(var.to_string(), val.trim().to_string());
            }
        }
    }

    let expected_mods = format_hyprland_mods_set(shortcut.modifiers);

    // 2. Scan `bind = ...` lines
    for raw_line in config_text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        let Some((directive, rest)) = line.split_once('=') else {
            continue;
        };
        let directive = directive.trim();
        if !directive.starts_with("bind") || directive.starts_with("bindm") {
            continue;
        }

        let parts: Vec<&str> = rest.split(',').collect();
        if parts.len() < 3 {
            continue;
        }

        let raw_mods = parts[0].trim();
        let resolved_mods = variables
            .get(raw_mods)
            .map(|s| s.as_str())
            .unwrap_or(raw_mods);

        let parsed_mods = parse_hyprlang_mods_set(resolved_mods);
        if parsed_mods != expected_mods {
            continue;
        }

        let key_str = parts[1].trim();
        if !matches_hyprland_key(key_str, shortcut.key) {
            continue;
        }

        let dispatcher = parts[2].trim();
        if dispatcher == "exec" {
            let cmd = parts[3..].join(",").trim().to_string();
            if is_pookie_command(&cmd) {
                return HyprlandBindingDiagnosis::VerifiedPookie;
            } else {
                return HyprlandBindingDiagnosis::Conflict {
                    command: format!("exec {cmd}"),
                };
            }
        } else {
            let arg = parts.get(3).map(|s| s.trim()).unwrap_or("");
            let cmd = if arg.is_empty() {
                dispatcher.to_string()
            } else {
                format!("{dispatcher} {arg}")
            };
            return HyprlandBindingDiagnosis::Conflict { command: cmd };
        }
    }

    HyprlandBindingDiagnosis::NotFound
}

fn format_hyprland_mods_set(mods: ShortcutModifiers) -> std::collections::BTreeSet<&'static str> {
    let mut set = std::collections::BTreeSet::new();
    if mods.super_key {
        set.insert("SUPER");
    }
    if mods.control {
        set.insert("CTRL");
    }
    if mods.alt {
        set.insert("ALT");
    }
    if mods.shift {
        set.insert("SHIFT");
    }
    set
}

fn parse_hyprlang_mods_set(mods_str: &str) -> std::collections::BTreeSet<&'static str> {
    let mut set = std::collections::BTreeSet::new();
    for token in mods_str.split([' ', '+', '|']) {
        let t = token.trim();
        if t.eq_ignore_ascii_case("super")
            || t.eq_ignore_ascii_case("mod4")
            || t.eq_ignore_ascii_case("win")
        {
            set.insert("SUPER");
        } else if t.eq_ignore_ascii_case("ctrl") || t.eq_ignore_ascii_case("control") {
            set.insert("CTRL");
        } else if t.eq_ignore_ascii_case("alt") || t.eq_ignore_ascii_case("mod1") {
            set.insert("ALT");
        } else if t.eq_ignore_ascii_case("shift") {
            set.insert("SHIFT");
        }
    }
    set
}

fn find_hyprland_socket() -> Option<PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;

    if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
        let candidate = PathBuf::from(xdg)
            .join("hypr")
            .join(&sig)
            .join(".socket.sock");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let fallback = PathBuf::from("/tmp/hypr").join(sig).join(".socket.sock");
    if fallback.exists() {
        return Some(fallback);
    }

    None
}

fn read_hyprland_config_file() -> Option<String> {
    let xdg_config = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        });

    let mut candidates = Vec::new();

    if let Some(ref config_root) = xdg_config {
        candidates.push(config_root.join("hypr/hyprland.lua"));
        candidates.push(config_root.join("hypr/hyprland.conf"));
    }

    candidates.push(PathBuf::from("/etc/hypr/hyprland.conf"));

    for candidate in candidates {
        if let Ok(content) = std::fs::read_to_string(&candidate) {
            return Some(content);
        }
    }

    None
}
