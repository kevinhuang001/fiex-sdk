//! Information and preview response DTOs.
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod info;
pub use info::*;

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
    fn info_presentation_uses_invisible_subsections_and_rejects_old_section_wire_keys() {
        let presentation: InfoProviderPresentation = serde_json::from_value(serde_json::json!({
            "title": "Media",
            "subsections": [{
                "id": "audio",
                "match": { "extensions": ["mp3"], "mimeTypes": ["audio/*"] },
                "fields": [{ "id": "duration", "label": "Duration", "compute": "onDemand" }]
            }]
        }))
        .unwrap();
        assert_eq!(presentation.title, "Media");
        assert_eq!(presentation.subsections[0].id, "audio");
        assert_eq!(
            presentation.subsections[0]
                .r#match
                .as_ref()
                .unwrap()
                .extensions,
            ["mp3"]
        );

        assert!(
            serde_json::from_value::<InfoProviderPresentation>(serde_json::json!({
                "title": "Media",
                "sections": [{ "id": "audio", "title": "Audio", "fields": [] }]
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<InfoSubsectionDescriptor>(serde_json::json!({
                "id": "audio",
                "title": "Audio",
                "fields": []
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<InfoFieldKey>(serde_json::json!({
                "sectionId": "audio",
                "fieldId": "duration"
            }))
            .is_err()
        );
    }

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
        assert!(
            serde_json::from_value::<PreviewView>(serde_json::json!({
                "kind": "html",
                "unexpected": true
            }))
            .is_err()
        );
    }
}
