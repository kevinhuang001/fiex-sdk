//! Standard event DTOs shared with Core.
//!
//! All Core system events receive one [`EventContext`] with a `targets` array.
//! The JSON-RPC `method` determines the event. Targets contain opaque refs and
//! optional, permission-projected metadata/read capabilities, never host paths.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRef {
    pub mount_id: String,
    pub node_id: String,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopedRead {
    pub resource_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContext {
    #[serde(rename = "ref")]
    pub reference: ResourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<ScopedRead>,
}

impl ResourceContext {
    /// File name inside the operation scope. Safe to pass to
    /// [`crate::Files::open_read`]; it is not an absolute host path.
    pub fn name(&self) -> Option<&str> {
        self.metadata.as_ref().map(|value| value.name.as_str())
    }
}

/// Common system-event parameters; the RPC method identifies the event.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventContext {
    pub session_id: String,
    pub targets: Vec<ResourceContext>,
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
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub rows: Vec<InfoRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoResponse {
    #[serde(default)]
    pub sections: Vec<InfoSection>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageView {
    pub source: String,
    pub entry: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
                read: None,
            }],
        };
        assert_eq!(context.targets[0].name(), Some("a.txt"));
        let encoded = serde_json::to_value(&context).unwrap();
        assert_eq!(encoded["targets"][0]["ref"]["nodeId"], "opaque");
        assert!(encoded["targets"][0].get("read").is_none());
        assert!(encoded.get("resource").is_none());
    }

    #[test]
    fn preview_html_is_package_confined() {
        let view = PreviewView::html("ui/preview.html");
        assert_eq!(view.kind, "html");
        assert_eq!(view.view.unwrap().entry, "ui/preview.html");
    }
}
