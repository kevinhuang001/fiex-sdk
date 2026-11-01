//! Trusted, permission-projected provider event context DTO.
use crate::{ProviderError, ProviderInput, ResourceContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Data a provider may inspect. Core populates this from trusted resource
/// resolution and permission projection; it never contains host paths or
/// bearer credentials.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRequestContext {
    pub method: String,
    #[serde(skip)]
    pub input: ProviderInput,
    #[serde(skip)]
    pub input_error: Option<String>,
    #[serde(default)]
    pub targets: Vec<ResourceContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<ResourceContext>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session_id: String,
    /// Original cardinality is retained for blank-canvas and bounded
    /// multi-selection decisions, even when a provider receives a reduced
    /// metadata projection.
    #[serde(default)]
    pub selection_count: usize,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub locale: String,
}

impl ProviderRequestContext {
    pub fn from_params(
        method: impl Into<String>,
        params: &Value,
        locale: impl Into<String>,
    ) -> Self {
        let method = method.into();
        let targets: Vec<ResourceContext> = params
            .get("targets")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| serde_json::from_value(item.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        let directory = params
            .get("directory")
            .filter(|value| value.is_object())
            .and_then(|value| serde_json::from_value(value.clone()).ok());
        let selection_count = params
            .get("selectionCount")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(targets.len());
        let session_id = params
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let (input, input_error) = match ProviderInput::decode(&method, params) {
            Ok(input) => (input, None),
            Err(error) => (ProviderInput::Unspecified, Some(error)),
        };
        Self {
            method,
            input,
            input_error,
            targets,
            directory,
            session_id,
            selection_count,
            locale: locale.into(),
        }
    }

    pub fn target(&self) -> Option<&ResourceContext> {
        self.targets.first()
    }

    pub fn selection_count(&self) -> usize {
        self.selection_count
    }

    pub fn input(&self) -> Result<&ProviderInput, ProviderError> {
        if let Some(error) = &self.input_error {
            return Err(ProviderError::invalid_params(error.clone()));
        }
        Ok(&self.input)
    }
}
