use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::app_paths;
use crate::shortcut_backend::{NamedKey, Shortcut, ShortcutKey, ShortcutModifiers};

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
}
