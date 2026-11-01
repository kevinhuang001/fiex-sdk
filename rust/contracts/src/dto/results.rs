use serde::{Deserialize, Serialize};

/// Result of a hook invocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HookResult {
    pub handled: bool,
}

/// Toolbar invocations have no consumer-defined return payload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolbarResult {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionValidation {
    pub valid: bool,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub credentials: std::collections::BTreeMap<String, String>,
}

impl ConnectionValidation {
    pub fn valid() -> Self {
        Self {
            valid: true,
            credentials: Default::default(),
        }
    }

    pub fn invalid() -> Self {
        Self {
            valid: false,
            credentials: Default::default(),
        }
    }

    pub fn credential(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.credentials.insert(name.into(), value.into());
        self
    }
}
