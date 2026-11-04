//! Connection event DTOs shared by Core and plugin providers.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ConnectionFieldValue {
    String(String),
    Boolean(bool),
    Number(serde_json::Number),
    /// Core's effective network allowlist is added to connection config as a
    /// string array; declared user-editable fields remain scalar values.
    Strings(Vec<String>),
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct ConnectionSettings(pub BTreeMap<String, ConnectionFieldValue>);

impl ConnectionSettings {
    pub fn to_value(&self) -> Result<Value, String> {
        serde_json::to_value(self).map_err(|error| error.to_string())
    }
}

impl std::ops::Deref for ConnectionSettings {
    type Target = BTreeMap<String, ConnectionFieldValue>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ConnectionSettings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

impl ConnectionRequest {
    pub fn connection_value(&self) -> Result<Option<Value>, String> {
        self.connection
            .as_ref()
            .map(ConnectionSettings::to_value)
            .transpose()
    }
}
