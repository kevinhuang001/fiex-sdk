//! Provider error behavior shared by native Core implementations and SDK plugins.
use crate::dto::ProviderError;

impl ProviderError {
    pub fn method_not_found(method: &str) -> Self {
        Self { code: -32601, message: format!("Unsupported method: {method}"), data: None }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self { code: -32602, message: message.into(), data: None }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self { code: -32603, message: message.into(), data: None }
    }

    pub fn cancelled() -> Self {
        Self { code: -32800, message: "task cancelled".into(), data: None }
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ProviderError {}
