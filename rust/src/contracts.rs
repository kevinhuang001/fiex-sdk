//! Standard event DTOs shared with Core.
//!
//! `info.resolve` receives an [`InfoContext`] and returns an
//! [`InfoResponse`]; `preview.open` receives a [`PreviewContext`] and returns
//! a [`PreviewView`]. Contexts contain opaque resource references and optional
//! metadata/read capabilities, never a host path.
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoContext {
    #[serde(default)]
    pub version: u32,
    pub session_id: String,
    #[serde(rename = "intent", default)]
    pub intent: String,
    pub resource: ResourceContext,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewContext {
    #[serde(default)]
    pub version: u32,
    pub session_id: String,
    #[serde(rename = "intent", default)]
    pub intent: String,
    pub resource: ResourceContext,
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
    pub title: Option<String>,
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
    pub fn html(entry: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            kind: "html".into(),
            title: Some(title.into()),
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
        let context = InfoContext {
            version: 1,
            session_id: "s".into(),
            intent: "info".into(),
            resource: ResourceContext {
                reference: ResourceRef {
                    mount_id: "local".into(),
                    node_id: "opaque".into(),
                },
                metadata: Some(ResourceMetadata {
                    name: "a.txt".into(),
                    kind: "file".into(),
                    extension: Some("txt".into()),
                    size: Some(1),
                    modified: None,
                }),
                read: None,
            },
        };
        assert_eq!(context.resource.name(), Some("a.txt"));
        assert!(serde_json::to_value(&context).unwrap()["resource"]
            .get("read")
            .is_none());
    }

    #[test]
    fn preview_html_is_package_confined() {
        let view = PreviewView::html("ui/preview.html", "Preview");
        assert_eq!(view.kind, "html");
        assert_eq!(view.view.unwrap().entry, "ui/preview.html");
    }
}
