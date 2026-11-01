use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Provider failure payload shared by Core-native handlers and SDK plugins.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}
