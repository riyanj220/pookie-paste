use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryContentRef<'a> {
    Text(&'a str),

    Image { file_path: &'a str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryItemError {
    UnsupportedContentType(String),

    MissingTextContent,

    UnexpectedTextFilePath,

    MissingImageFilePath,

    UnexpectedImageTextContent,

    InvalidImagePath(String),
}

impl fmt::Display for HistoryItemError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedContentType(content_type) => {
                write!(
                    formatter,
                    "unsupported history content type: {content_type}"
                )
            }

            Self::MissingTextContent => {
                write!(formatter, "text history item is missing text content")
            }

            Self::UnexpectedTextFilePath => {
                write!(
                    formatter,
                    "text history item unexpectedly contains a file reference"
                )
            }

            Self::MissingImageFilePath => {
                write!(
                    formatter,
                    "image history item is missing its file reference"
                )
            }

            Self::UnexpectedImageTextContent => {
                write!(
                    formatter,
                    "image history item unexpectedly contains text content"
                )
            }

            Self::InvalidImagePath(path) => {
                write!(formatter, "invalid image history reference: {path}")
            }
        }
    }
}

impl std::error::Error for HistoryItemError {}

///
/// Lightweight clipboard-history metadata transferred over
/// Pookie's IPC socket.
///
/// Image bytes are deliberately NOT part of this structure.
///
/// Image entries contain only an application-owned relative
/// file reference such as:
///
/// ```text
/// images/<uuid>.png
/// ```
///
/// The UI resolves and loads that file locally when it needs
/// to render the thumbnail.
///
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,

    pub content_type: String,

    pub text_content: Option<String>,

    pub file_path: Option<String>,

    pub created_at: String,

    #[serde(default)]
    pub pinned_at: Option<String>,
}

impl HistoryItem {
    pub fn text(id: String, text_content: String, created_at: String) -> Self {
        Self {
            id,

            content_type: "text".to_string(),

            text_content: Some(text_content),

            file_path: None,

            created_at,

            pinned_at: None,
        }
    }

    pub fn image(
        id: String,
        file_path: String,
        created_at: String,
    ) -> Result<Self, HistoryItemError> {
        validate_image_path(&file_path)?;

        Ok(Self {
            id,

            content_type: "image".to_string(),

            text_content: None,

            file_path: Some(file_path),

            created_at,

            pinned_at: None,
        })
    }

    pub fn with_pinned_at(mut self, pinned_at: Option<String>) -> Self {
        self.pinned_at = pinned_at;
        self
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned_at.is_some()
    }

    pub fn is_text(&self) -> bool {
        self.content_type == "text"
    }

    pub fn is_image(&self) -> bool {
        self.content_type == "image"
    }

    /// Return the typed content reference represented by this
    /// IPC history item.
    ///
    /// This validates the wire-level invariants before the UI
    /// consumes the item.
    pub fn content(&self) -> Result<HistoryContentRef<'_>, HistoryItemError> {
        match self.content_type.as_str() {
            "text" => {
                if self.file_path.is_some() {
                    return Err(HistoryItemError::UnexpectedTextFilePath);
                }

                let Some(text) = self.text_content.as_deref() else {
                    return Err(HistoryItemError::MissingTextContent);
                };

                Ok(HistoryContentRef::Text(text))
            }

            "image" => {
                if self.text_content.is_some() {
                    return Err(HistoryItemError::UnexpectedImageTextContent);
                }

                let Some(file_path) = self.file_path.as_deref() else {
                    return Err(HistoryItemError::MissingImageFilePath);
                };

                validate_image_path(file_path)?;

                Ok(HistoryContentRef::Image { file_path })
            }

            other => Err(HistoryItemError::UnsupportedContentType(other.to_string())),
        }
    }
}

///
/// Validate Pookie's IPC-level image reference.
///
/// IPC only permits application-relative image references:
///
/// ```text
/// images/<safe-id>.png
/// ```
///
/// It deliberately rejects:
///
/// ```text
/// /absolute/path
/// ../escape
/// images/nested/file.png
/// arbitrary extensions
/// ```
///
fn validate_image_path(file_path: &str) -> Result<(), HistoryItemError> {
    let mut parts = file_path.split('/');

    let directory = parts.next();

    let filename = parts.next();

    if parts.next().is_some() {
        return Err(HistoryItemError::InvalidImagePath(file_path.to_string()));
    }

    if directory != Some("images") {
        return Err(HistoryItemError::InvalidImagePath(file_path.to_string()));
    }

    let Some(filename) = filename else {
        return Err(HistoryItemError::InvalidImagePath(file_path.to_string()));
    };

    let Some(item_id) = filename.strip_suffix(".png") else {
        return Err(HistoryItemError::InvalidImagePath(file_path.to_string()));
    };

    if item_id.is_empty()
        || !item_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(HistoryItemError::InvalidImagePath(file_path.to_string()));
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum IpcFocusTarget {
    X11(u64),

    Kde(String),

    Sway(i64),

    Hyprland(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcRequest {
    Ping,

    GetHistory,

    CaptureFocusTarget,

    ActivateItem {
        id: String,

        target_id: Option<IpcFocusTarget>,
    },

    DeleteItem {
        id: String,
    },

    TogglePinItem {
        id: String,
    },

    ClearHistory,

    ToggleUi,

    GetShortcutStatus,

    ReloadConfig,

    SetShortcut {
        modifiers: Vec<String>,
        key: String,
    },

    ConfigurePortalShortcut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcResponse {
    Pong,

    History { items: Vec<HistoryItem> },

    FocusTarget { target_id: Option<IpcFocusTarget> },

    Activated { outcome: ActivationOutcome },

    Deleted { deleted: bool },

    PinToggled { id: String, is_pinned: bool },

    Cleared { count: u64 },

    UiToggled { launched: bool },

    ShortcutStatus { status: ShortcutStatusInfo },

    ConfigReloaded { status: ShortcutStatusInfo },

    Error { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcShortcutCapability {
    Native,
    Portal,
    CompositorManaged,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcCompositorBindingStatus {
    /// Binding exists in active compositor and target is verified as Pookie.
    Verified,
    /// Binding definitely exists, but its target cannot be verified (e.g. Hyprland __lua callback).
    BoundUnverified,
    /// No binding exists for the configured shortcut.
    Unconfigured,
    /// Binding exists and is known to target something else.
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IpcShortcutState {
    Initializing,
    Active {
        description: String,
    },
    CompositorManaged {
        binding_status: IpcCompositorBindingStatus,
        snippet: String,
        conflict: Option<String>,
        diagnostic: Option<String>,
    },
    Conflict {
        details: String,
    },
    Unavailable {
        reason: String,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutStatusInfo {
    pub configured_shortcut: String,
    pub backend_name: Option<String>,
    pub capability: Option<IpcShortcutCapability>,
    pub effective_shortcut: Option<String>,
    pub state: IpcShortcutState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationOutcome {
    Pasted,

    ClipboardUpdated,

    PasteFailed,

    NotFound,

    UnsupportedContent,
}

#[cfg(test)]
mod tests {
    use super::{
        ActivationOutcome, HistoryContentRef, HistoryItem, HistoryItemError, IpcFocusTarget,
        IpcRequest, IpcResponse, IpcShortcutCapability, IpcShortcutState, ShortcutStatusInfo,
    };

    #[test]
    fn text_history_item_exposes_text_content() {
        let item = HistoryItem::text(
            "item-1".to_string(),
            "hello".to_string(),
            "2026-09-17T10:00:00Z".to_string(),
        );

        assert_eq!(item.content(), Ok(HistoryContentRef::Text("hello",),),);
    }

    #[test]
    fn image_history_item_exposes_relative_reference() {
        let item = HistoryItem::image(
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            "images/550e8400-e29b-41d4-a716-446655440000.png".to_string(),
            "2026-09-17T10:00:00Z".to_string(),
        )
        .expect("image history item creation failed");

        assert_eq!(
            item.content(),
            Ok(HistoryContentRef::Image {
                file_path: "images/550e8400-e29b-41d4-a716-446655440000.png",
            },),
        );
    }

    #[test]
    fn rejects_absolute_image_path() {
        let result = HistoryItem::image(
            "item-1".to_string(),
            "/tmp/image.png".to_string(),
            "2026-09-17T10:00:00Z".to_string(),
        );

        assert!(matches!(result, Err(HistoryItemError::InvalidImagePath(_))));
    }

    #[test]
    fn rejects_traversal_image_path() {
        let result = HistoryItem::image(
            "item-1".to_string(),
            "images/../image.png".to_string(),
            "2026-09-17T10:00:00Z".to_string(),
        );

        assert!(matches!(result, Err(HistoryItemError::InvalidImagePath(_))));
    }

    #[test]
    fn rejects_image_item_with_text_payload() {
        let item = HistoryItem {
            id: "item-1".to_string(),

            content_type: "image".to_string(),

            text_content: Some("unexpected".to_string()),

            file_path: Some("images/550e8400-e29b-41d4-a716-446655440000.png".to_string()),

            created_at: "2026-09-17T10:00:00Z".to_string(),

            pinned_at: None,
        };

        assert_eq!(
            item.content(),
            Err(HistoryItemError::UnexpectedImageTextContent,),
        );
    }

    #[test]
    fn rejects_text_item_with_file_reference() {
        let item = HistoryItem {
            id: "item-1".to_string(),

            content_type: "text".to_string(),

            text_content: Some("hello".to_string()),

            file_path: Some("images/550e8400-e29b-41d4-a716-446655440000.png".to_string()),

            created_at: "2026-09-17T10:00:00Z".to_string(),

            pinned_at: None,
        };

        assert_eq!(
            item.content(),
            Err(HistoryItemError::UnexpectedTextFilePath,),
        );
    }

    #[test]
    fn activate_item_request_with_x11_target_round_trips() {
        let request = IpcRequest::ActivateItem {
            id: "item-123".to_string(),

            target_id: Some(IpcFocusTarget::X11(12345)),
        };

        let encoded = serde_json::to_string(&request).expect("serialization failed");

        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("deserialization failed");

        assert_eq!(decoded, request);
    }

    #[test]
    fn activate_item_request_with_kde_target_round_trips() {
        let request = IpcRequest::ActivateItem {
            id: "item-123".to_string(),

            target_id: Some(IpcFocusTarget::Kde(
                "f97159ad-a2d7-4cfb-aa57-d39940229877".to_string(),
            )),
        };

        let encoded = serde_json::to_string(&request).expect("serialization failed");

        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("deserialization failed");

        assert_eq!(decoded, request);
    }

    #[test]
    fn activate_item_request_without_target_round_trips() {
        let request = IpcRequest::ActivateItem {
            id: "item-123".to_string(),

            target_id: None,
        };

        let encoded = serde_json::to_string(&request).expect("serialization failed");

        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("deserialization failed");

        assert_eq!(decoded, request);
    }

    #[test]
    fn activated_response_round_trips() {
        let response = IpcResponse::Activated {
            outcome: ActivationOutcome::Pasted,
        };

        let encoded = serde_json::to_string(&response).expect("serialization failed");

        let decoded: IpcResponse = serde_json::from_str(&encoded).expect("deserialization failed");

        assert_eq!(decoded, response);
    }

    #[test]
    fn activate_item_serializes_x11_target() {
        let request = IpcRequest::ActivateItem {
            id: "item-123".to_string(),

            target_id: Some(IpcFocusTarget::X11(12345)),
        };

        let encoded = serde_json::to_string(&request).expect("serialization failed");

        assert_eq!(
            encoded,
            r#"{"type":"activate_item","id":"item-123","target_id":{"kind":"x11","value":12345}}"#
        );
    }

    #[test]
    fn activate_item_serializes_kde_target() {
        let request = IpcRequest::ActivateItem {
            id: "item-123".to_string(),

            target_id: Some(IpcFocusTarget::Kde(
                "12345678-1234-5678-1234-567812345678".to_string(),
            )),
        };

        let encoded = serde_json::to_string(&request).expect("serialization failed");

        assert_eq!(
            encoded,
            r#"{"type":"activate_item","id":"item-123","target_id":{"kind":"kde","value":"12345678-1234-5678-1234-567812345678"}}"#
        );
    }

    #[test]
    fn capture_focus_target_request_round_trips() {
        let request = IpcRequest::CaptureFocusTarget;

        let encoded = serde_json::to_string(&request).expect("serialization failed");

        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("deserialization failed");

        assert_eq!(decoded, request);
    }

    #[test]
    fn x11_focus_target_response_round_trips() {
        let response = IpcResponse::FocusTarget {
            target_id: Some(IpcFocusTarget::X11(12345)),
        };

        let encoded = serde_json::to_string(&response).expect("serialization failed");

        let decoded: IpcResponse = serde_json::from_str(&encoded).expect("deserialization failed");

        assert_eq!(decoded, response);
    }

    #[test]
    fn kde_focus_target_response_round_trips() {
        let response = IpcResponse::FocusTarget {
            target_id: Some(IpcFocusTarget::Kde(
                "12345678-1234-5678-1234-567812345678".to_string(),
            )),
        };

        let encoded = serde_json::to_string(&response).expect("serialization failed");

        let decoded: IpcResponse = serde_json::from_str(&encoded).expect("deserialization failed");

        assert_eq!(decoded, response);
    }

    #[test]
    fn sway_focus_target_response_round_trips() {
        let response = IpcResponse::FocusTarget {
            target_id: Some(IpcFocusTarget::Sway(42)),
        };

        let encoded = serde_json::to_string(&response).expect("serialization failed");
        assert_eq!(
            encoded,
            r#"{"type":"focus_target","target_id":{"kind":"sway","value":42}}"#
        );

        let decoded: IpcResponse = serde_json::from_str(&encoded).expect("deserialization failed");
        assert_eq!(decoded, response);
    }

    #[test]
    fn hyprland_focus_target_response_round_trips() {
        let response = IpcResponse::FocusTarget {
            target_id: Some(IpcFocusTarget::Hyprland("0x55a72f1b8a90".to_string())),
        };

        let encoded = serde_json::to_string(&response).expect("serialization failed");
        assert_eq!(
            encoded,
            r#"{"type":"focus_target","target_id":{"kind":"hyprland","value":"0x55a72f1b8a90"}}"#
        );

        let decoded: IpcResponse = serde_json::from_str(&encoded).expect("deserialization failed");
        assert_eq!(decoded, response);
    }

    #[test]
    fn toggle_pin_item_request_round_trips() {
        let request = IpcRequest::TogglePinItem {
            id: "item-123".to_string(),
        };

        let encoded = serde_json::to_string(&request).expect("serialization failed");

        assert_eq!(encoded, r#"{"type":"toggle_pin_item","id":"item-123"}"#);

        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("deserialization failed");

        assert_eq!(decoded, request);
    }

    #[test]
    fn pin_toggled_response_round_trips() {
        let response = IpcResponse::PinToggled {
            id: "item-123".to_string(),
            is_pinned: true,
        };

        let encoded = serde_json::to_string(&response).expect("serialization failed");

        assert_eq!(
            encoded,
            r#"{"type":"pin_toggled","id":"item-123","is_pinned":true}"#
        );

        let decoded: IpcResponse = serde_json::from_str(&encoded).expect("deserialization failed");

        assert_eq!(decoded, response);
    }

    #[test]
    fn history_item_deserializes_without_pinned_at_field() {
        let json = r#"{
            "id": "legacy-item",
            "content_type": "text",
            "text_content": "legacy text",
            "file_path": null,
            "created_at": "2026-09-17T10:00:00Z"
        }"#;

        let decoded: HistoryItem = serde_json::from_str(json).expect("deserialization failed");

        assert_eq!(decoded.pinned_at, None);
        assert!(!decoded.is_pinned());
    }

    #[test]
    fn history_item_with_pinned_at_round_trips() {
        let item = HistoryItem::text(
            "pinned-item".to_string(),
            "hello pinned".to_string(),
            "2026-09-17T10:00:00Z".to_string(),
        )
        .with_pinned_at(Some("2026-09-20T12:00:00Z".to_string()));

        assert!(item.is_pinned());

        let encoded = serde_json::to_string(&item).expect("serialization failed");
        let decoded: HistoryItem = serde_json::from_str(&encoded).expect("deserialization failed");

        assert_eq!(decoded, item);
        assert_eq!(decoded.pinned_at.as_deref(), Some("2026-09-20T12:00:00Z"));
    }

    #[test]
    fn toggle_ui_request_round_trips() {
        let request = IpcRequest::ToggleUi;
        let encoded = serde_json::to_string(&request).expect("serialization failed");
        assert_eq!(encoded, r#"{"type":"toggle_ui"}"#);
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("deserialization failed");
        assert_eq!(decoded, request);
    }

    #[test]
    fn reload_config_request_round_trips() {
        let request = IpcRequest::ReloadConfig;
        let encoded = serde_json::to_string(&request).expect("serialization failed");
        assert_eq!(encoded, r#"{"type":"reload_config"}"#);
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("deserialization failed");
        assert_eq!(decoded, request);
    }

    #[test]
    fn config_reloaded_response_round_trips() {
        let response = IpcResponse::ConfigReloaded {
            status: ShortcutStatusInfo {
                configured_shortcut: "Super+V".to_string(),
                backend_name: Some("X11 global shortcut".to_string()),
                capability: Some(IpcShortcutCapability::Native),
                effective_shortcut: Some("Super+V".to_string()),
                state: IpcShortcutState::Active {
                    description: "X11 root window grab for Super+V".to_string(),
                },
            },
        };
        let encoded = serde_json::to_string(&response).expect("serialization failed");
        assert!(encoded.contains(r#""type":"config_reloaded""#));
        let decoded: IpcResponse = serde_json::from_str(&encoded).expect("deserialization failed");
        assert_eq!(decoded, response);
    }

    #[test]
    fn set_shortcut_request_round_trips() {
        let request = IpcRequest::SetShortcut {
            modifiers: vec!["Ctrl".to_string(), "Shift".to_string()],
            key: "P".to_string(),
        };
        let encoded = serde_json::to_string(&request).expect("serialization failed");
        assert!(encoded.contains(r#""type":"set_shortcut""#));
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("deserialization failed");
        assert_eq!(decoded, request);
    }

    #[test]
    fn configure_portal_shortcut_request_round_trips() {
        let request = IpcRequest::ConfigurePortalShortcut;
        let encoded = serde_json::to_string(&request).expect("serialization failed");
        assert_eq!(encoded, r#"{"type":"configure_portal_shortcut"}"#);
        let decoded: IpcRequest = serde_json::from_str(&encoded).expect("deserialization failed");
        assert_eq!(decoded, request);
    }
}
