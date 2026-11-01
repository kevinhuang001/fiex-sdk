//! Safe request projection and process-local invocation boundary.
use crate::{ProviderError, ProviderInput, ResourceCapabilities, ResourceContext, ResourceLocation};
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
    pub fn from_params(method: impl Into<String>, params: &Value, locale: impl Into<String>) -> Self {
        let method = method.into();
        let targets: Vec<ResourceContext> = params
            .get("targets")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(|item| serde_json::from_value(item.clone()).ok()).collect())
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
        let session_id = params.get("sessionId").and_then(Value::as_str).unwrap_or_default().to_owned();
        let (input, input_error) = match ProviderInput::decode(&method, params) {
            Ok(input) => (input, None),
            Err(error) => (ProviderInput::Unspecified, Some(error)),
        };
        Self { method, input, input_error, targets, directory, session_id, selection_count, locale: locale.into() }
    }

    pub fn target(&self) -> Option<&ResourceContext> { self.targets.first() }

    pub fn selection_count(&self) -> usize { self.selection_count }

    pub fn input(&self) -> Result<&ProviderInput, ProviderError> {
        if let Some(error) = &self.input_error {
            return Err(ProviderError::invalid_params(error.clone()));
        }
        Ok(&self.input)
    }
}

/// One invocation interface used by both shared provider traits and their
/// native/plugin implementations. The opaque handle is strictly process-local
/// and is never serialized by the contract crate.
pub trait ProviderInvocation: Send + Sync {
    fn request(&self) -> &ProviderRequestContext;
    fn local_service_handle(&self) -> Option<&dyn Any> { None }

    fn method(&self) -> &str { &self.request().method }
    fn input(&self) -> Result<&ProviderInput, ProviderError> { self.request().input() }
    fn targets(&self) -> &[ResourceContext] { &self.request().targets }
    fn target(&self) -> Option<&ResourceContext> { self.request().target() }
    fn directory(&self) -> Option<&ResourceContext> { self.request().directory.as_ref() }
    fn selection_count(&self) -> usize { self.request().selection_count() }
    fn view_location(&self) -> Option<&ResourceLocation> {
        self.target()
            .or_else(|| self.directory())
            .and_then(|resource| resource.view.as_ref())
    }
    fn storage_location(&self) -> Option<&ResourceLocation> {
        self.target()
            .or_else(|| self.directory())
            .and_then(|resource| resource.storage.as_ref())
    }
    fn resource_capabilities(&self) -> Option<&ResourceCapabilities> {
        self.target()
            .or_else(|| self.directory())
            .map(|resource| &resource.capabilities)
    }
    fn locale_tag(&self) -> &str { &self.request().locale }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ResourceRef, ResourceType};
    use serde_json::json;

    #[test]
    fn request_context_preserves_blank_selection_and_trusted_directory_location() {
        let context = ProviderRequestContext::from_params(
            "contextMenu.resolve",
            &json!({
                "selectionCount": 0,
                "targets": [],
                "directory": {
                    "ref": {"mountId":"applications", "nodeId":"root"},
                    "view": {
                        "type":"vfs",
                        "providerId":"core:applications",
                        "mountId":"core:applications"
                    },
                    "storage": {
                        "type":"vfs",
                        "providerId":"core:applications",
                        "mountId":"core:applications"
                    },
                    "capabilities": {"operations":["list", "mkdir"]}
                }
            }),
            "en-US",
        );
        assert_eq!(context.selection_count(), 0);
        let directory = context.directory.as_ref().unwrap();
        assert_eq!(directory.view.as_ref().unwrap().resource_type, ResourceType::Vfs);
        assert_eq!(directory.view.as_ref().unwrap().provider_id, "core:applications");
        assert!(directory.capabilities.allows(crate::FsOperation::Mkdir));
        assert_eq!(directory.reference, ResourceRef {
            mount_id: "applications".into(),
            node_id: "root".into(),
        });
    }
}
