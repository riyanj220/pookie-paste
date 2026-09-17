use ipc::{HistoryItem, IpcFocusTarget};

use storage::StoredClipboardItem;

use uuid::Uuid;

use crate::focus_backend::FocusTarget;

pub fn to_history_item(item: StoredClipboardItem) -> HistoryItem {
    match item.content_type.as_str() {
        "text" => {
            HistoryItem {
                id: item.id,

                content_type: "text".to_string(),

                text_content: item.text_content,

                /*
                 * Never expose a file reference for a text
                 * IPC history item even if a malformed
                 * legacy database row somehow contains one.
                 */
                file_path: None,

                created_at: item.created_at,
            }
        }

        "image" => {
            HistoryItem {
                id: item.id,

                content_type: "image".to_string(),

                /*
                 * Image IPC entries never transport image
                 * bytes or textual payloads.
                 */
                text_content: None,

                /*
                 * Preserve the application-relative
                 * reference written by ImageStore:
                 *
                 * images/<uuid>.png
                 */
                file_path: item.file_path,

                created_at: item.created_at,
            }
        }

        /*
         * Storage currently only permits Pookie's v1
         * content types in normal operation.
         *
         * Preserve unknown metadata rather than inventing
         * another content type. HistoryItem::content()
         * will reject it safely if such a legacy/corrupt
         * row ever reaches a consumer.
         */
        _ => HistoryItem {
            id: item.id,

            content_type: item.content_type,

            text_content: item.text_content,

            file_path: item.file_path,

            created_at: item.created_at,
        },
    }
}

pub fn to_ipc_focus_target(target: FocusTarget) -> IpcFocusTarget {
    match target {
        FocusTarget::X11(id) => IpcFocusTarget::X11(id),

        FocusTarget::Kde(id) => IpcFocusTarget::Kde(id.to_string()),
    }
}

pub fn from_ipc_focus_target(target: IpcFocusTarget) -> Result<FocusTarget, String> {
    match target {
        IpcFocusTarget::X11(id) => Ok(FocusTarget::x11(id)),

        IpcFocusTarget::Kde(value) => {
            let id = Uuid::parse_str(&value)
                .map_err(|error| format!("invalid KDE focus target UUID {value}: {error}"))?;

            Ok(FocusTarget::kde(id))
        }
    }
}

#[cfg(test)]
mod tests {
    use ipc::{HistoryContentRef, HistoryItemError};

    use super::*;

    #[test]
    fn maps_text_storage_item_without_file_reference() {
        let item = StoredClipboardItem {
            id: "text-1".to_string(),

            content_type: "text".to_string(),

            text_content: Some("hello".to_string()),

            /*
             * Simulate malformed storage metadata.
             *
             * IPC must not expose a file reference for
             * a text item.
             */
            file_path: Some("images/should-not-leak.png".to_string()),

            content_hash: "hash".to_string(),

            created_at: "2026-09-17T10:00:00Z".to_string(),
        };

        let mapped = to_history_item(item);

        assert_eq!(mapped.file_path, None,);

        assert_eq!(mapped.content(), Ok(HistoryContentRef::Text("hello",),),);
    }

    #[test]
    fn maps_image_storage_item_as_reference_only() {
        let item = StoredClipboardItem {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),

            content_type: "image".to_string(),

            /*
             * Simulate malformed unexpected text.
             *
             * IPC image mapping must strip it.
             */
            text_content: Some("must not leak".to_string()),

            file_path: Some("images/550e8400-e29b-41d4-a716-446655440000.png".to_string()),

            content_hash: "image-hash".to_string(),

            created_at: "2026-09-17T10:00:00Z".to_string(),
        };

        let mapped = to_history_item(item);

        assert_eq!(mapped.text_content, None,);

        assert_eq!(
            mapped.content(),
            Ok(HistoryContentRef::Image {
                file_path: "images/550e8400-e29b-41d4-a716-446655440000.png",
            },),
        );
    }

    #[test]
    fn image_mapping_contains_no_encoded_image_payload() {
        let item = StoredClipboardItem {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),

            content_type: "image".to_string(),

            text_content: None,

            file_path: Some("images/550e8400-e29b-41d4-a716-446655440000.png".to_string()),

            content_hash: "image-hash".to_string(),

            created_at: "2026-09-17T10:00:00Z".to_string(),
        };

        let mapped = to_history_item(item);

        let json = serde_json::to_string(&mapped).expect("history item serialization failed");

        assert!(json.contains("images/550e8400-e29b-41d4-a716-446655440000.png",));

        assert!(!json.contains("image_bytes",));

        assert!(!json.contains("base64",));
    }

    #[test]
    fn malformed_image_path_is_rejected_by_ipc_content_validation() {
        let item = StoredClipboardItem {
            id: "image-1".to_string(),

            content_type: "image".to_string(),

            text_content: None,

            file_path: Some("/tmp/not-owned.png".to_string()),

            content_hash: "image-hash".to_string(),

            created_at: "2026-09-17T10:00:00Z".to_string(),
        };

        let mapped = to_history_item(item);

        assert!(matches!(
            mapped.content(),
            Err(HistoryItemError::InvalidImagePath(_))
        ));
    }

    #[test]
    fn maps_x11_focus_target_to_ipc() {
        assert_eq!(
            to_ipc_focus_target(FocusTarget::x11(42,),),
            IpcFocusTarget::X11(42,),
        );
    }

    #[test]
    fn maps_kde_focus_target_to_ipc() {
        let id = Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("valid UUID");

        assert_eq!(
            to_ipc_focus_target(FocusTarget::kde(id,),),
            IpcFocusTarget::Kde(id.to_string(),),
        );
    }

    #[test]
    fn maps_x11_ipc_target_to_focus_target() {
        let target =
            from_ipc_focus_target(IpcFocusTarget::X11(42)).expect("mapping should succeed");

        assert_eq!(target, FocusTarget::x11(42,),);
    }

    #[test]
    fn maps_kde_ipc_target_to_focus_target() {
        let id = Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("valid UUID");

        let target = from_ipc_focus_target(IpcFocusTarget::Kde(id.to_string()))
            .expect("mapping should succeed");

        assert_eq!(target, FocusTarget::kde(id,),);
    }

    #[test]
    fn rejects_invalid_kde_uuid() {
        let result = from_ipc_focus_target(IpcFocusTarget::Kde("not-a-uuid".to_string()));

        assert!(result.is_err());
    }
}
