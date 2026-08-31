use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub content_type: String,
    pub text_content: Option<String>,
    pub file_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcRequest {
    Ping,
    GetHistory,
    ActivateItem { id: String },
    DeleteItem { id: String },
    ClearHistory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcResponse {
    Pong,

    History { items: Vec<HistoryItem> },

    Activated { outcome: ActivationOutcome },

    Deleted { deleted: bool },

    Cleared { count: u64 },

    Error { message: String },
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
    use super::{ActivationOutcome, IpcRequest, IpcResponse};

    #[test]
    fn activate_item_request_round_trips() {
        let request = IpcRequest::ActivateItem {
            id: "item-123".to_string(),
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
}
