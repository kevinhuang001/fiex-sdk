//! Provider request DTOs for standard event families.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A transport-neutral request. No direct host path, authentication token,
/// plugin ID, or user-derived ownership claim is included here: those values
/// belong to the authorized Core invocation scope, not provider input.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRequest {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceEventRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InfoComputeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub fields: Vec<crate::InfoFieldKey>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextMenuResolveRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextMenuExecuteRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionMenuResolveRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionSummary>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionMenuExecuteRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionSummary>,
}

/// Credentials are projected out before a connection request is sent. The
/// remaining summary is descriptive data only and never grants a capability.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionSummary {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub plugin_id: String,
    #[serde(default)]
    pub mount_path: String,
    #[serde(default)]
    pub saved: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub plugin_view: bool,
    #[serde(default)]
    pub vfs: bool,
}
