use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::app_paths;
use crate::shortcut_backend::{NamedKey, Shortcut, ShortcutKey, ShortcutModifiers};

pub const DEFAULT_CONFIG_TEMPLATE: &str = r#"# Pookie Paste Configuration File
# Documentation: https://github.com/riyanj220/pookie-paste

[shortcut.primary]
modifiers = ["SUPER"]
key = "V"
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutConfigError {
    FileNotFound(PathBuf),
    Io(String),
    Parse(String),
    ModifierRequired(String),
    UnknownModifier(String),
    InvalidKey(String),
}

impl fmt::Display for ShortcutConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileNotFound(path) => {
                write!(f, "configuration file not found: {}", path.display())
            }
            Self::Io(err) => write!(f, "failed reading configuration file: {err}"),
            Self::Parse(err) => write!(f, "failed to parse configuration TOML: {err}"),
            Self::ModifierRequired(key) => write!(
                f,
                "shortcut must include at least one modifier key (Super, Ctrl, Alt, or Shift); cannot bind bare key '{key}'"
            ),
            Self::UnknownModifier(m) => write!(f, "unknown shortcut modifier: '{m}'"),
            Self::InvalidKey(k) => write!(f, "invalid or unsupported shortcut key: '{k}'"),
        }
    }
}

impl std::error::Error for ShortcutConfigError {}

/// Top-level configuration file schema for Pookie Paste.
///
/// Supports standard layout:
/// ```toml
/// [shortcut.primary]
/// modifiers = ["SUPER"]
/// key = "V"
/// ```
///
/// As well as single-table convenience:
/// ```toml
/// [shortcut]
/// modifiers = "SUPER"
/// key = "V"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShortcutConfig {
    #[serde(default)]
    pub shortcut: ShortcutSection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShortcutSection {
    #[serde(default)]
    pub primary: Option<KeyBindingConfig>,

    // Convenience fallbacks if user placed keys directly under [shortcut]
    #[serde(default)]
    pub modifiers: Option<ModifiersConfig>,

    #[serde(default)]
    pub key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBindingConfig {
    #[serde(default = "default_modifiers")]
    pub modifiers: ModifiersConfig,

    #[serde(default = "default_key")]
    pub key: String,
}

impl Default for KeyBindingConfig {
    fn default() -> Self {
        Self {
            modifiers: default_modifiers(),
            key: default_key(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModifiersConfig {
    Single(String),
    Multiple(Vec<String>),
}

fn default_modifiers() -> ModifiersConfig {
    ModifiersConfig::Multiple(vec!["SUPER".to_string()])
}

fn default_key() -> String {
    "V".to_string()
}

impl ShortcutConfig {
    /// Loads configuration from the standard XDG configuration path,
    /// or returns safe defaults if the file does not exist or fails validation.
    pub fn load_or_default() -> Self {
        let path = match app_paths::config_path() {
            Ok(path) => path,
            Err(err) => {
                debug!("could not resolve config path: {err}; using defaults");
                return Self::default();
            }
        };

        Self::load_or_default_from_path(&path)
    }

    /// Resolves or bootstraps the target configuration file path.
    pub fn config_path() -> std::io::Result<PathBuf> {
        Self::ensure_config_file_exists()
    }

    /// Bootstraps the default configuration file if no configuration file currently exists.
    ///
    /// Respects any existing configuration file already resolved by `app_paths::config_path()`
    /// (including legacy fallback paths such as `~/.config/pookie/config.toml`).
    /// If no configuration file exists anywhere, creates the default file at
    /// `app_paths::default_config_path()` atomically using `create_new(true)` to prevent TOCTOU races.
    pub fn ensure_config_file_exists() -> std::io::Result<PathBuf> {
        let resolved = app_paths::config_path()?;
        let default_path = app_paths::default_config_path()?;
        Self::ensure_config_file_exists_at(&resolved, &default_path)
    }

    /// Bootstraps default configuration file at `default_path` if neither `resolved`
    /// nor `default_path` exists on disk.
    pub fn ensure_config_file_exists_at(
        resolved: &Path,
        default_path: &Path,
    ) -> std::io::Result<PathBuf> {
        if resolved.exists() {
            return Ok(resolved.to_path_buf());
        }

        if let Some(parent) = default_path.parent() {
            fs::create_dir_all(parent)?;
        }

        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(default_path)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(DEFAULT_CONFIG_TEMPLATE.as_bytes())?;
                file.flush()?;
                info!(
                    path = %default_path.display(),
                    "bootstrapped default configuration file"
                );
                Ok(default_path.to_path_buf())
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                // Concurrently created by another thread or process
                Ok(default_path.to_path_buf())
            }
            Err(err) => Err(err),
        }
    }

    /// Loads configuration strictly for runtime reload.
    ///
    /// - If no resolved configuration file exists (e.g. user deleted it to reset defaults),
    ///   re-bootstraps the canonical default configuration file using `ensure_config_file_exists()`,
    ///   and strictly loads the newly created file.
    /// - If the configuration file exists but contains invalid TOML, invalid keys,
    ///   or encounters an I/O error, returns an error so that the current runtime
    ///   shortcut is safely preserved.
    pub fn load_strict() -> Result<Self, ShortcutConfigError> {
        let resolved = app_paths::config_path()
            .map_err(|e| ShortcutConfigError::Io(format!("could not resolve config path: {e}")))?;
        let default_path = app_paths::default_config_path().map_err(|e| {
            ShortcutConfigError::Io(format!("could not resolve default config path: {e}"))
        })?;

        Self::load_strict_with_paths(&resolved, &default_path)
    }

    /// Loads configuration strictly using explicit resolved and default paths.
    pub fn load_strict_with_paths(
        resolved: &Path,
        default_path: &Path,
    ) -> Result<Self, ShortcutConfigError> {
        let target_path = if resolved.exists() {
            resolved.to_path_buf()
        } else {
            Self::ensure_config_file_exists_at(resolved, default_path).map_err(|e| {
                ShortcutConfigError::Io(format!(
                    "could not bootstrap configuration file during reload: {e}"
                ))
            })?
        };

        Self::load_from_path(&target_path)
    }

    /// Loads configuration strictly from a specified file path.
    pub fn load_strict_from_path(path: &Path) -> Result<Self, ShortcutConfigError> {
        Self::load_from_path(path)
    }

    /// Loads configuration from a specified file path,
    /// or returns safe defaults if the file does not exist or fails validation.
    pub fn load_or_default_from_path(path: &Path) -> Self {
        if !path.exists() {
            debug!(
                "config file not found at {}; using defaults",
                path.display()
            );
            return Self::default();
        }

        match Self::load_from_path(path) {
            Ok(config) => config,
            Err(err) => {
                warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to parse config file; falling back to default shortcut (Super+V)"
                );
                Self::default()
            }
        }
    }

    /// Loads and validates configuration from an explicit file path.
    pub fn load_from_path(path: &Path) -> Result<Self, ShortcutConfigError> {
        if !path.exists() {
            return Err(ShortcutConfigError::FileNotFound(path.to_path_buf()));
        }

        let content = fs::read_to_string(path)
            .map_err(|e| ShortcutConfigError::Io(format!("{}: {e}", path.display())))?;

        Self::parse_str(&content)
    }

    /// Parses and validates configuration content from a TOML string.
    pub fn parse_str(toml_str: &str) -> Result<Self, ShortcutConfigError> {
        let config: Self =
            toml::from_str(toml_str).map_err(|e| ShortcutConfigError::Parse(e.to_string()))?;

        // Validate that primary shortcut can be converted into a valid runtime Shortcut
        let _ = config.primary_shortcut()?;

        Ok(config)
    }

    /// Resolves the primary keybinding configuration.
    pub fn primary_binding(&self) -> KeyBindingConfig {
        if let Some(primary) = &self.shortcut.primary {
            return primary.clone();
        }

        if let (Some(modifiers), Some(key)) = (&self.shortcut.modifiers, &self.shortcut.key) {
            return KeyBindingConfig {
                modifiers: modifiers.clone(),
                key: key.clone(),
            };
        }

        KeyBindingConfig::default()
    }

    /// Converts the configured primary shortcut into the internal `Shortcut` model.
    pub fn primary_shortcut(&self) -> Result<Shortcut, ShortcutConfigError> {
        let binding = self.primary_binding();
        binding.to_shortcut()
    }

    /// Updates only the shortcut configuration inside an existing TOML string using format-preserving
    /// editing, retaining comments, whitespace, and unrelated sections.
    pub fn update_shortcut_in_toml_str(
        existing_content: &str,
        shortcut: Shortcut,
    ) -> Result<String, ShortcutConfigError> {
        use toml_edit::{Array, DocumentMut, Item, Table, Value, value};

        let mut doc: DocumentMut = if existing_content.trim().is_empty() {
            DEFAULT_CONFIG_TEMPLATE
                .parse()
                .map_err(|e: toml_edit::TomlError| ShortcutConfigError::Parse(e.to_string()))?
        } else {
            existing_content
                .parse()
                .map_err(|e: toml_edit::TomlError| ShortcutConfigError::Parse(e.to_string()))?
        };

        let mut mods = Vec::new();
        if shortcut.modifiers.super_key {
            mods.push("SUPER");
        }
        if shortcut.modifiers.control {
            mods.push("CTRL");
        }
        if shortcut.modifiers.alt {
            mods.push("ALT");
        }
        if shortcut.modifiers.shift {
            mods.push("SHIFT");
        }

        let key_str = shortcut.key.to_string();

        let mut arr = Array::new();
        for m in mods {
            arr.push(m);
        }

        if let Some(table) = doc.get_mut("shortcut").and_then(|i| i.as_table_like_mut()) {
            if table.contains_key("primary") || !table.contains_key("key") {
                let primary_item = table.entry("primary").or_insert(Item::Table(Table::new()));
                if let Some(primary_table) = primary_item.as_table_like_mut() {
                    primary_table.insert("modifiers", Item::Value(Value::Array(arr)));
                    primary_table.insert("key", value(key_str));
                }
            } else {
                table.insert("modifiers", Item::Value(Value::Array(arr)));
                table.insert("key", value(key_str));
            }
        } else {
            let mut primary = Table::new();
            primary.insert("modifiers", Item::Value(Value::Array(arr)));
            primary.insert("key", value(key_str));
            let mut shortcut_table = Table::new();
            shortcut_table.insert("primary", Item::Table(primary));
            doc.insert("shortcut", Item::Table(shortcut_table));
        }

        let updated = doc.to_string();
        // Validate that updated content is valid ShortcutConfig
        let _ = Self::parse_str(&updated)?;
        Ok(updated)
    }

    /// Prepares an updated configuration in a sibling temporary file within the same directory,
    /// preserving existing permissions if the target file exists.
    pub fn prepare_new_config_file(
        target_path: &Path,
        shortcut: Shortcut,
    ) -> Result<PathBuf, ShortcutConfigError> {
        let parent = target_path.parent().ok_or_else(|| {
            ShortcutConfigError::Io(format!("no parent directory for {}", target_path.display()))
        })?;
        fs::create_dir_all(parent).map_err(|e| {
            ShortcutConfigError::Io(format!(
                "failed creating directory {}: {e}",
                parent.display()
            ))
        })?;

        let existing_content = if target_path.exists() {
            fs::read_to_string(target_path)
                .map_err(|e| ShortcutConfigError::Io(format!("{}: {e}", target_path.display())))?
        } else {
            String::new()
        };

        let updated_content = Self::update_shortcut_in_toml_str(&existing_content, shortcut)?;

        let temp_filename = format!(
            ".{}.tmp.{}",
            target_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("config.toml"),
            uuid::Uuid::new_v4()
        );
        let temp_path = parent.join(temp_filename);

        fs::write(&temp_path, updated_content.as_bytes())
            .map_err(|e| ShortcutConfigError::Io(format!("{}: {e}", temp_path.display())))?;

        if let Ok(metadata) = fs::metadata(target_path) {
            let _ = fs::set_permissions(&temp_path, metadata.permissions());
        }

        Ok(temp_path)
    }

    /// Atomically commits a prepared temporary file over the target configuration file.
    pub fn commit_prepared_file(
        temp_path: &Path,
        target_path: &Path,
    ) -> Result<(), ShortcutConfigError> {
        fs::rename(temp_path, target_path).map_err(|e| {
            let _ = fs::remove_file(temp_path);
            ShortcutConfigError::Io(format!(
                "failed committing {} to {}: {e}",
                temp_path.display(),
                target_path.display()
            ))
        })
    }

    /// Cleans up a prepared temporary file on failure.
    pub fn clean_prepared_file(temp_path: &Path) {
        let _ = fs::remove_file(temp_path);
    }
}

impl KeyBindingConfig {
    pub fn to_shortcut(&self) -> Result<Shortcut, ShortcutConfigError> {
        let modifiers = parse_modifiers(&self.modifiers)?;
        let key = parse_key(&self.key)?;

        if !modifiers.has_any() {
            return Err(ShortcutConfigError::ModifierRequired(self.key.clone()));
        }

        Ok(Shortcut::new(key, modifiers))
    }
}

fn parse_modifiers(config: &ModifiersConfig) -> Result<ShortcutModifiers, ShortcutConfigError> {
    let mut modifiers = ShortcutModifiers::NONE;

    let items: Vec<String> = match config {
        ModifiersConfig::Single(s) => s.split('+').map(|part| part.trim().to_string()).collect(),
        ModifiersConfig::Multiple(list) => {
            let mut flattened = Vec::new();
            for item in list {
                for part in item.split('+') {
                    flattened.push(part.trim().to_string());
                }
            }
            flattened
        }
    };

    for item in items {
        if item.is_empty() {
            continue;
        }

        match item.to_ascii_lowercase().as_str() {
            "super" | "mod4" | "logo" | "win" => modifiers.super_key = true,
            "ctrl" | "control" => modifiers.control = true,
            "alt" | "mod1" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            unknown => return Err(ShortcutConfigError::UnknownModifier(unknown.to_string())),
        }
    }

    Ok(modifiers)
}

fn parse_key(key_str: &str) -> Result<ShortcutKey, ShortcutConfigError> {
    let trimmed = key_str.trim();

    if trimmed.is_empty() {
        return Err(ShortcutConfigError::InvalidKey(
            "key cannot be empty".to_string(),
        ));
    }

    // Check named keys
    match trimmed.to_ascii_lowercase().as_str() {
        "space" => return Ok(ShortcutKey::Named(NamedKey::Space)),
        "tab" => return Ok(ShortcutKey::Named(NamedKey::Tab)),
        "enter" | "return" => return Ok(ShortcutKey::Named(NamedKey::Enter)),
        "esc" | "escape" => return Ok(ShortcutKey::Named(NamedKey::Escape)),
        "ins" | "insert" => return Ok(ShortcutKey::Named(NamedKey::Insert)),
        "del" | "delete" => return Ok(ShortcutKey::Named(NamedKey::Delete)),
        "f1" => return Ok(ShortcutKey::Named(NamedKey::F(1))),
        "f2" => return Ok(ShortcutKey::Named(NamedKey::F(2))),
        "f3" => return Ok(ShortcutKey::Named(NamedKey::F(3))),
        "f4" => return Ok(ShortcutKey::Named(NamedKey::F(4))),
        "f5" => return Ok(ShortcutKey::Named(NamedKey::F(5))),
        "f6" => return Ok(ShortcutKey::Named(NamedKey::F(6))),
        "f7" => return Ok(ShortcutKey::Named(NamedKey::F(7))),
        "f8" => return Ok(ShortcutKey::Named(NamedKey::F(8))),
        "f9" => return Ok(ShortcutKey::Named(NamedKey::F(9))),
        "f10" => return Ok(ShortcutKey::Named(NamedKey::F(10))),
        "f11" => return Ok(ShortcutKey::Named(NamedKey::F(11))),
        "f12" => return Ok(ShortcutKey::Named(NamedKey::F(12))),
        _ => {}
    }

    // Check single character
    let mut chars = trimmed.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_ascii_alphanumeric()
    {
        return Ok(ShortcutKey::Character(c.to_ascii_lowercase()));
    }

    Err(ShortcutConfigError::InvalidKey(trimmed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_resolves_to_super_v() {
        let config = ShortcutConfig::default();
        let shortcut = config.primary_shortcut().expect("failed resolving default");

        assert_eq!(shortcut, Shortcut::super_v());
        assert_eq!(shortcut.to_string(), "Super+V");
    }

    #[test]
    fn parses_nested_primary_table_with_array_modifiers() {
        let toml = r#"
[shortcut.primary]
modifiers = ["SUPER", "SHIFT"]
key = "P"
"#;
        let config = ShortcutConfig::parse_str(toml).expect("parse failed");
        let shortcut = config.primary_shortcut().expect("conversion failed");

        assert_eq!(shortcut.key, ShortcutKey::Character('p'));
        assert!(shortcut.modifiers.super_key);
        assert!(shortcut.modifiers.shift);
        assert!(!shortcut.modifiers.control);
        assert_eq!(shortcut.to_string(), "Super+Shift+P");
    }

    #[test]
    fn parses_single_table_with_string_modifier() {
        let toml = r#"
[shortcut]
modifiers = "CTRL+ALT"
key = "Space"
"#;
        let config = ShortcutConfig::parse_str(toml).expect("parse failed");
        let shortcut = config.primary_shortcut().expect("conversion failed");

        assert_eq!(shortcut.key, ShortcutKey::Named(NamedKey::Space));
        assert!(shortcut.modifiers.control);
        assert!(shortcut.modifiers.alt);
        assert!(!shortcut.modifiers.super_key);
        assert_eq!(shortcut.to_string(), "Ctrl+Alt+Space");
    }

    #[test]
    fn parses_function_keys_case_insensitively() {
        let toml = r#"
[shortcut.primary]
modifiers = "super"
key = "F12"
"#;
        let config = ShortcutConfig::parse_str(toml).expect("parse failed");
        let shortcut = config.primary_shortcut().expect("conversion failed");

        assert_eq!(shortcut.key, ShortcutKey::Named(NamedKey::F(12)));
        assert!(shortcut.modifiers.super_key);
        assert_eq!(shortcut.to_string(), "Super+F12");
    }

    #[test]
    fn rejects_modifier_less_bare_keys_for_safety() {
        let toml = r#"
[shortcut.primary]
modifiers = []
key = "V"
"#;
        let err = ShortcutConfig::parse_str(toml).expect_err("should reject bare key");
        assert!(matches!(err, ShortcutConfigError::ModifierRequired(_)));
    }

    #[test]
    fn rejects_unknown_modifier() {
        let toml = r#"
[shortcut.primary]
modifiers = ["INVALID"]
key = "V"
"#;
        let err = ShortcutConfig::parse_str(toml).expect_err("should reject unknown modifier");
        assert!(matches!(err, ShortcutConfigError::UnknownModifier(_)));
    }

    #[test]
    fn rejects_unknown_key_string() {
        let toml = r#"
[shortcut.primary]
modifiers = ["SUPER"]
key = "UnknownLongKeyName"
"#;
        let err = ShortcutConfig::parse_str(toml).expect_err("should reject unknown key");
        assert!(matches!(err, ShortcutConfigError::InvalidKey(_)));
    }

    #[test]
    fn loads_or_defaults_gracefully_when_nonexistent() {
        let nonexistent = Path::new("/nonexistent/path/pookie-paste/config.toml");
        let config = ShortcutConfig::load_or_default_from_path(nonexistent);
        let sc = config.primary_shortcut().expect("default resolution");
        assert_eq!(sc, Shortcut::super_v());
    }

    #[test]
    fn default_config_template_is_valid_and_resolves_to_super_v() {
        let config = ShortcutConfig::parse_str(DEFAULT_CONFIG_TEMPLATE).expect("template is valid");
        let shortcut = config.primary_shortcut().expect("converts to shortcut");
        assert_eq!(shortcut, Shortcut::super_v());
    }

    #[test]
    fn load_strict_from_path_missing_returns_file_not_found() {
        let nonexistent = Path::new("/nonexistent/path/to/missing_config.toml");
        let err = ShortcutConfig::load_strict_from_path(nonexistent)
            .expect_err("missing explicit path must return error");
        assert!(matches!(err, ShortcutConfigError::FileNotFound(_)));
    }

    #[test]
    fn load_strict_with_paths_bootstraps_missing_file_and_loads_default() {
        let temp_dir =
            std::env::temp_dir().join(format!("pookie_bootstrap_test_{}", uuid::Uuid::new_v4()));
        let default_path = temp_dir.join("pookie-paste").join("config.toml");
        let resolved = default_path.clone();

        assert!(!default_path.exists());

        let config = ShortcutConfig::load_strict_with_paths(&resolved, &default_path)
            .expect("should bootstrap and load default configuration");
        let shortcut = config.primary_shortcut().expect("primary shortcut");
        assert_eq!(shortcut, Shortcut::super_v());

        // Invariant: canonical editable configuration file exists on disk
        assert!(default_path.exists());
        let content = fs::read_to_string(&default_path).unwrap();
        assert_eq!(content, DEFAULT_CONFIG_TEMPLATE);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn load_strict_with_paths_preserves_existing_legacy_config() {
        use std::io::Write;
        let temp_dir =
            std::env::temp_dir().join(format!("pookie_legacy_test_{}", uuid::Uuid::new_v4()));
        let legacy_dir = temp_dir.join("pookie");
        fs::create_dir_all(&legacy_dir).unwrap();
        let legacy_file = legacy_dir.join("config.toml");
        let default_path = temp_dir.join("pookie-paste").join("config.toml");

        let mut f = fs::File::create(&legacy_file).unwrap();
        writeln!(
            f,
            "[shortcut.primary]\nmodifiers = ['CTRL', 'SHIFT']\nkey = 'P'"
        )
        .unwrap();

        let config = ShortcutConfig::load_strict_with_paths(&legacy_file, &default_path)
            .expect("should load legacy config without error");
        let shortcut = config.primary_shortcut().expect("primary shortcut");
        assert_eq!(shortcut.to_string(), "Ctrl+Shift+P");

        // Canonical default path must NOT have been created, preserving legacy config
        assert!(!default_path.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn load_strict_with_paths_malformed_fails_and_preserves_file_unmodified() {
        use std::io::Write;
        let temp_dir =
            std::env::temp_dir().join(format!("pookie_malformed_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let config_file = temp_dir.join("config.toml");

        let malformed_content = "[shortcut\nnot valid toml = true";
        let mut f = fs::File::create(&config_file).unwrap();
        f.write_all(malformed_content.as_bytes()).unwrap();

        let err = ShortcutConfig::load_strict_with_paths(&config_file, &config_file)
            .expect_err("malformed config must fail strictly");
        assert!(matches!(err, ShortcutConfigError::Parse(_)));

        // File must not be overwritten or modified
        let current_content = fs::read_to_string(&config_file).unwrap();
        assert_eq!(current_content, malformed_content);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn load_strict_from_path_malformed_returns_parse_error() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir().join(format!("pookie_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let file_path = temp_dir.join("malformed.toml");
        let mut f = fs::File::create(&file_path).unwrap();
        writeln!(f, "[shortcut\nthis is not valid toml").unwrap();

        let err = ShortcutConfig::load_strict_from_path(&file_path)
            .expect_err("malformed config must fail strictly");
        assert!(matches!(err, ShortcutConfigError::Parse(_)));

        let _ = fs::remove_file(file_path);
        let _ = fs::remove_dir(temp_dir);
    }

    #[test]
    fn load_strict_from_path_invalid_key_returns_validation_error() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir().join(format!("pookie_test_key_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let file_path = temp_dir.join("invalid_key.toml");
        let mut f = fs::File::create(&file_path).unwrap();
        writeln!(
            f,
            "[shortcut.primary]\nmodifiers = ['SUPER']\nkey = 'NoSuchKey123'"
        )
        .unwrap();

        let err = ShortcutConfig::load_strict_from_path(&file_path)
            .expect_err("invalid key config must fail strictly");
        assert!(matches!(err, ShortcutConfigError::InvalidKey(_)));

        let _ = fs::remove_file(file_path);
        let _ = fs::remove_dir(temp_dir);
    }

    #[test]
    fn update_shortcut_preserves_comments_and_unrelated_sections() {
        let original = r#"# User configuration header comment
# Author: Test User

[shortcut.primary]
modifiers = ["SUPER"]
key = "V"

# Settings for a future feature
[something.future]
enabled = true
threshold = 42
"#;

        let new_shortcut = Shortcut::new(
            ShortcutKey::Character('p'),
            ShortcutModifiers {
                control: true,
                shift: true,
                ..ShortcutModifiers::NONE
            },
        );

        let updated = ShortcutConfig::update_shortcut_in_toml_str(original, new_shortcut).unwrap();

        assert!(updated.contains("# User configuration header comment"));
        assert!(updated.contains("# Author: Test User"));
        assert!(updated.contains("# Settings for a future feature"));
        assert!(updated.contains("[something.future]"));
        assert!(updated.contains("enabled = true"));
        assert!(updated.contains("threshold = 42"));

        let parsed = ShortcutConfig::parse_str(&updated).unwrap();
        assert_eq!(
            parsed.primary_shortcut().unwrap().to_string(),
            "Ctrl+Shift+P"
        );
    }

    #[test]
    fn prepare_and_commit_config_file_atomically_updates_target() {
        let temp_dir = std::env::temp_dir().join(format!("pookie_atomic_{}", uuid::Uuid::new_v4()));
        let _ = fs::create_dir_all(&temp_dir);
        let target_file = temp_dir.join("config.toml");

        fs::write(
            &target_file,
            "# Original\n[shortcut.primary]\nmodifiers = ['SUPER']\nkey = 'V'\n",
        )
        .unwrap();

        let new_shortcut = Shortcut::new(
            ShortcutKey::Character('p'),
            ShortcutModifiers {
                control: true,
                shift: true,
                ..ShortcutModifiers::NONE
            },
        );

        let prepared_path =
            ShortcutConfig::prepare_new_config_file(&target_file, new_shortcut).unwrap();
        assert!(prepared_path.exists());
        assert_ne!(prepared_path, target_file);

        // Before commit, target file still has original content
        let before_commit = fs::read_to_string(&target_file).unwrap();
        assert!(before_commit.contains("modifiers = ['SUPER']"));

        // Commit prepared file
        ShortcutConfig::commit_prepared_file(&prepared_path, &target_file).unwrap();
        assert!(!prepared_path.exists());

        // Target file has new content and preserved comments
        let after_commit = fs::read_to_string(&target_file).unwrap();
        assert!(after_commit.contains("# Original"));
        assert!(after_commit.contains("P"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
