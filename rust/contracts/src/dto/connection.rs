//! Typed connection navigation result returned by a ConnectionProvider.

use crate::{PackageView, ResourceRef};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ConnectionOpenResult {
    Vfs {
        mount_id: String,
        node_ref: ResourceRef,
    },
    PluginView {
        view: PackageView,
        #[serde(default, skip_serializing_if = "Value::is_null")]
        state: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tab_title: Option<String>,
    },
}
