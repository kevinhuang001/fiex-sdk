//! Information and preview response DTOs.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InfoCompute {
    OnOpen,
    OnDemand,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InfoFieldDescriptor {
    pub id: String,
    pub label: String,
    pub compute: InfoCompute,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InfoSectionDescriptor {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub fields: Vec<InfoFieldDescriptor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InfoFieldKey {
    pub section_id: String,
    pub field_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InfoFieldResult {
    pub section_id: String,
    pub field_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub unavailable: bool,
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
    /// Native Core render data. Plugin responses may omit these fields; Core
    /// validates their provenance before placing any value in this DTO.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Resolved Core session fields are present only after the plugin view has
    /// passed package confinement and session authorization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contribution_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,
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
            html: None,
            path: None,
            name: None,
            mime: None,
            media: None,
            extension: None,
            truncated: None,
            size: None,
            entry: None,
            session_id: None,
            plugin_id: None,
            contribution_id: None,
            timeout_ms: None,
            context: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_html_is_package_confined() {
        let view = PreviewView::html("ui/preview.html");
        assert_eq!(view.kind, "html");
        assert_eq!(view.view.unwrap().entry, "ui/preview.html");
    }

    #[test]
    fn preview_dto_covers_native_render_fields_and_rejects_unknown_data() {
        let image: PreviewView = serde_json::from_value(serde_json::json!({
            "kind": "image",
            "path": "/brokered/image.png",
            "name": "image.png",
            "mime": "image/png"
        }))
        .unwrap();
        assert_eq!(image.kind, "image");
        assert_eq!(image.name.as_deref(), Some("image.png"));
        assert_eq!(image.path.as_deref(), Some("/brokered/image.png"));
        assert!(serde_json::from_value::<PreviewView>(serde_json::json!({
            "kind": "html",
            "unexpected": true
        }))
        .is_err());
    }
}
