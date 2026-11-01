//! Safe request projection and process-local invocation boundary.
use crate::ResourceContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;

/// Data a provider may inspect. Core must populate this from trusted resource
/// resolution and permission projection; it must never contain host paths or
/// bearer credentials.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRequestContext {
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub targets: Vec<ResourceContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<ResourceContext>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub locale: String,
}

impl ProviderRequestContext {
    pub fn from_params(method: impl Into<String>, params: Value, locale: impl Into<String>) -> Self {
        let targets = params
            .get("targets")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(|item| serde_json::from_value(item.clone()).ok()).collect())
            .unwrap_or_default();
        let directory = params
            .get("directory")
            .filter(|value| value.is_object())
            .and_then(|value| serde_json::from_value(value.clone()).ok());
        let session_id = params.get("sessionId").and_then(Value::as_str).unwrap_or_default().to_owned();
        Self { method: method.into(), params, targets, directory, session_id, locale: locale.into() }
    }

    pub fn target(&self) -> Option<&ResourceContext> { self.targets.first() }
}

/// One invocation interface used by both shared provider traits and their
/// native/plugin implementations. The opaque handle is strictly process-local
/// and is never serialized by the contract crate.
pub trait ProviderInvocation: Send + Sync {
    fn request(&self) -> &ProviderRequestContext;
    fn local_service_handle(&self) -> Option<&dyn Any> { None }

    fn method(&self) -> &str { &self.request().method }
    fn params(&self) -> &Value { &self.request().params }
    fn targets(&self) -> &[ResourceContext] { &self.request().targets }
    fn target(&self) -> Option<&ResourceContext> { self.request().target() }
    fn directory(&self) -> Option<&ResourceContext> { self.request().directory.as_ref() }
    fn locale_tag(&self) -> &str { &self.request().locale }
}
