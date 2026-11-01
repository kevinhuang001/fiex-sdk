//! Safe request projection and process-local invocation boundary.
use crate::{
    ProviderError, ProviderInput, ProviderRequestContext, ResourceCapabilities, ResourceContext,
    ResourceLocation,
};
use std::any::Any;

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
