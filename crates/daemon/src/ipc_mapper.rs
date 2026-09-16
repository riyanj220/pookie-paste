use ipc::{HistoryItem, IpcFocusTarget};

use storage::StoredClipboardItem;

use uuid::Uuid;

use crate::focus_backend::FocusTarget;

pub fn to_history_item(item: StoredClipboardItem) -> HistoryItem {
    HistoryItem {
        id: item.id,
        content_type: item.content_type,
        text_content: item.text_content,
        file_path: item.file_path,
        created_at: item.created_at,
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
    use super::*;

    #[test]
    fn maps_x11_focus_target_to_ipc() {
        assert_eq!(
            to_ipc_focus_target(FocusTarget::x11(42),),
            IpcFocusTarget::X11(42),
        );
    }

    #[test]
    fn maps_kde_focus_target_to_ipc() {
        let id = Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("valid UUID");

        assert_eq!(
            to_ipc_focus_target(FocusTarget::kde(id),),
            IpcFocusTarget::Kde(id.to_string(),),
        );
    }

    #[test]
    fn maps_x11_ipc_target_to_focus_target() {
        let target =
            from_ipc_focus_target(IpcFocusTarget::X11(42)).expect("mapping should succeed");

        assert_eq!(target, FocusTarget::x11(42),);
    }

    #[test]
    fn maps_kde_ipc_target_to_focus_target() {
        let id = Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("valid UUID");

        let target = from_ipc_focus_target(IpcFocusTarget::Kde(id.to_string()))
            .expect("mapping should succeed");

        assert_eq!(target, FocusTarget::kde(id),);
    }

    #[test]
    fn rejects_invalid_kde_uuid() {
        let result = from_ipc_focus_target(IpcFocusTarget::Kde("not-a-uuid".to_string()));

        assert!(result.is_err(),);
    }
}
