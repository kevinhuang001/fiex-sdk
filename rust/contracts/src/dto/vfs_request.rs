//! Typed VFS request DTO shared by Core and the SDK filesystem dispatcher.
use crate::{ConnectionSettings, SearchMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Typed storage request fields used by Core and the SDK's VFS dispatcher.
/// Stream chunk payloads remain private transport data in `FsServer`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VfsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_revision: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub ignores: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<SearchMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contribution_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_context: Option<BTreeMap<String, Value>>,
}

impl VfsRequest {
    pub fn connection_value(&self) -> Result<Option<Value>, String> {
        self.connection
            .as_ref()
            .map(ConnectionSettings::to_value)
            .transpose()
    }
}

impl VfsRequest {
    pub fn bounded_limit(&self, maximum: usize) -> usize {
        self.limit.unwrap_or(maximum).clamp(1, maximum)
    }
}
