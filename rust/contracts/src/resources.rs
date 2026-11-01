//! Standard event DTOs shared with Core.
//!
//! All Core system events receive one [`EventContext`] with a `targets` array.
//! The JSON-RPC `method` determines the event. Targets contain opaque refs and
//! optional, permission-projected metadata, never host paths.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Whether a resource is owned by an ordinary filesystem or a virtual
/// filesystem. This describes a trusted Core location, not a file's content
/// type and not the kind of Provider that contributed to an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    Fs,
    Vfs,
}

/// A Core-resolved location identity. `provider_id` and `mount_id` are
/// informational selectors only; they are never credentials or authority to
/// route file operations. Core must derive them from its live registries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceLocation {
    #[serde(rename = "type")]
    pub resource_type: ResourceType,
    pub provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_id: Option<String>,
}

/// Effective operations visible to a provider for this invocation. This is a
/// permission-projected snapshot for UI decisions; Core rechecks every
/// operation against the current scope and storage owner when it executes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceCapabilities {
    #[serde(default)]
    pub operations: Vec<crate::FsOperation>,
}

impl ResourceCapabilities {
    pub fn allows(&self, operation: crate::FsOperation) -> bool {
        self.operations.contains(&operation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRef {
    pub mount_id: String,
    pub node_id: String,
}

impl ResourceRef {
    /// Opaque argument for a Core-brokered executable. Core resolves it only
    /// inside the current scope; this never reveals a host filesystem path.
    pub fn command_arg(&self) -> String {
        format!(
            "fiex-ref:{}",
            serde_json::to_string(self).expect("resource ref serialization")
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMetadata {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContext {
    #[serde(rename = "ref")]
    pub reference: ResourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceMetadata>,
    /// Trusted presentation location. Recent aliases can differ from storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<ResourceLocation>,
    /// Trusted current storage owner, omitted when the source is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<ResourceLocation>,
    #[serde(default)]
    pub capabilities: ResourceCapabilities,
}

/// An event resource after Core has resolved its view, storage owner and
/// effective operations. The wire projection deliberately omits host paths
/// and storage credentials.
pub type ResolvedResourceContext = ResourceContext;

impl ResourceContext {
    /// Display name projected by Core. Use `reference` with `Files::open_read`
    /// when opening a target; a name does not carry authorization.
    pub fn name(&self) -> Option<&str> {
        self.metadata.as_ref().map(|value| value.name.as_str())
    }
}

/// Common system-event parameters; the RPC method identifies the event.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventContext {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session_id: String,
    pub targets: Vec<ResourceContext>,
}

/// A bounded page of Local SearchProvider results. Both Core's validator and
/// the plugin SDK serialize exactly this shape, including optional cursors.
/// Entries stay JSON because local filesystem and provider VFS entries use
/// different, separately validated storage-specific DTOs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub entries: Vec<Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl SearchResults {
    pub fn validate_page(&self, limit: usize) -> Result<(), String> {
        if self.entries.len() > limit {
            return Err("search provider exceeded requested page limit".into());
        }
        crate::pagination::validate_cursor(self.next_cursor.as_deref())
            .map_err(|_| "invalid search pagination cursor".to_string())

    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoRow {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub copyable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoSection {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,

    pub id: String,
    pub title: String,
    #[serde(default)]
    pub rows: Vec<InfoRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PackageView {
    pub source: String,
    pub entry: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PreviewView {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<PackageView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub state: Value,
}

impl PreviewView {
    pub fn html(entry: impl Into<String>) -> Self {
        Self {
            kind: "html".into(),
            view: Some(PackageView {
                source: "package".into(),
                entry: entry.into(),
            }),
            text: None,
            message: None,
            state: Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_projects_optional_metadata_and_read() {
        let context = EventContext {
            session_id: "s".into(),
            targets: vec![ResourceContext {
                reference: ResourceRef {
                    mount_id: "local".into(),
                    node_id: "opaque".into(),
                },
                metadata: Some(ResourceMetadata {
                    name: "a.txt".into(),
                    kind: "file".into(),
                    extension: Some("txt".into()),
                    mime: Some("text/plain".into()),
                    size: Some(1),
                    modified: None,
                }),
                view: None,
                storage: None,
                capabilities: ResourceCapabilities::default(),
            }],
        };
        assert_eq!(context.targets[0].name(), Some("a.txt"));
        let encoded = serde_json::to_value(&context).unwrap();
        assert_eq!(encoded["targets"][0]["ref"]["nodeId"], "opaque");
        assert!(encoded["targets"][0].get("read").is_none());
        assert!(encoded.get("resource").is_none());
    }

    #[test]
    fn resolved_resource_context_carries_trusted_locations_and_operations() {
        let context = ResourceContext {
            reference: ResourceRef {
                mount_id: "recent".into(),
                node_id: "opaque-alias".into(),
            },
            metadata: None,
            view: Some(ResourceLocation {
                resource_type: ResourceType::Vfs,
                provider_id: "core:recent".into(),
                mount_id: Some("core:recent".into()),
            }),
            storage: Some(ResourceLocation {
                resource_type: ResourceType::Fs,
                provider_id: "core:local-fs".into(),
                mount_id: Some("local".into()),
            }),
            capabilities: ResourceCapabilities {
                operations: vec![crate::FsOperation::Read, crate::FsOperation::Stat],
            },
        };
        let wire = serde_json::to_value(&context).unwrap();
        assert_eq!(wire["view"]["type"], "vfs");
        assert_eq!(wire["view"]["providerId"], "core:recent");
        assert_eq!(wire["storage"]["type"], "fs");
        assert_eq!(wire["capabilities"]["operations"][0], "read");
        assert!(wire.get("path").is_none());
    }

    #[test]
    fn search_page_rejects_wrong_flag_and_excessive_cursor() {
        let page = SearchResults {
            entries: vec![serde_json::json!({"path":"file"})],
            ..Default::default()
        };
        assert!(page.validate_page(1).is_ok());
        assert!(page.validate_page(0).is_err());
        let serialized = serde_json::to_value(&page).unwrap();
        assert!(serialized.get("truncated").is_none());
        assert!(serialized.get("nextCursor").is_none());
        assert!(serde_json::from_value::<SearchResults>(
            serde_json::json!({"entries":[], "truncated":"false"})
        ).is_err());
        assert!(serde_json::from_value::<SearchResults>(
            serde_json::json!({"entries":[], "truncated":null})
        ).is_err());
        let malformed = SearchResults {
            next_cursor: Some("x".repeat(2049)),
            ..Default::default()
        };
        assert!(malformed.validate_page(1).is_err());
    }

    #[test]
    fn preview_html_is_package_confined() {
        let view = PreviewView::html("ui/preview.html");
        assert_eq!(view.kind, "html");
        assert_eq!(view.view.unwrap().entry, "ui/preview.html");
    }
}
