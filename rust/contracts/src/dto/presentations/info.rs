use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InfoSubsectionMatch {
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub mime_types: Vec<String>,
    #[serde(default)]
    pub kinds: Vec<String>,
}

impl InfoSubsectionMatch {
    /// Match trusted resource metadata without reading file contents. Values in
    /// each dimension are alternatives; populated dimensions are combined with
    /// AND so a declaration can require, for example, both a file kind and an
    /// `.mp3` extension.
    pub fn matches(&self, metadata: &crate::ResourceMetadata) -> bool {
        let extension = metadata
            .extension
            .as_deref()
            .unwrap_or_default()
            .trim_start_matches('.');
        let mime = metadata.mime.as_deref().unwrap_or_default();
        (self.extensions.is_empty()
            || self.extensions.iter().any(|value| {
                value
                    .trim_start_matches('.')
                    .eq_ignore_ascii_case(extension)
            }))
            && (self.mime_types.is_empty()
                || self
                    .mime_types
                    .iter()
                    .any(|pattern| info_mime_matches(pattern, mime)))
            && (self.kinds.is_empty()
                || self
                    .kinds
                    .iter()
                    .any(|kind| kind.eq_ignore_ascii_case(&metadata.kind)))
    }
}

fn info_mime_matches(pattern: &str, mime: &str) -> bool {
    if pattern.eq_ignore_ascii_case(mime) {
        return true;
    }
    let Some((kind, "*")) = pattern.split_once('/') else {
        return false;
    };
    mime.split_once('/')
        .is_some_and(|(actual_kind, _)| kind.eq_ignore_ascii_case(actual_kind))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InfoSubsectionDescriptor {
    pub id: String,
    #[serde(default)]
    pub r#match: Option<InfoSubsectionMatch>,
    #[serde(default)]
    pub fields: Vec<InfoFieldDescriptor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InfoProviderPresentation {
    pub title: String,
    #[serde(default)]
    pub subsections: Vec<InfoSubsectionDescriptor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InfoFieldKey {
    pub subsection_id: String,
    pub field_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InfoFieldResult {
    pub subsection_id: String,
    pub field_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub unavailable: bool,
}

#[cfg(test)]
mod info_match_tests {
    use super::*;

    #[test]
    fn subsection_match_uses_or_within_dimensions_and_and_between_them() {
        let rule = InfoSubsectionMatch {
            extensions: vec!["mp3".into(), "flac".into()],
            mime_types: vec!["audio/*".into()],
            kinds: vec!["file".into()],
        };
        let metadata = crate::ResourceMetadata {
            kind: "file".into(),
            extension: Some(".MP3".into()),
            mime: Some("AUDIO/MPEG".into()),
            ..Default::default()
        };
        assert!(rule.matches(&metadata));

        let mut metadata = metadata.clone();
        metadata.extension = Some("mp4".into());
        assert!(!rule.matches(&metadata));
        metadata.extension = Some("mp3".into());
        metadata.mime = Some("video/mp4".into());
        assert!(!rule.matches(&metadata));
        metadata.mime = Some("audio/mpeg".into());
        metadata.kind = "directory".into();
        assert!(!rule.matches(&metadata));
    }

    #[test]
    fn empty_dimensions_do_not_restrict_metadata() {
        let metadata = crate::ResourceMetadata {
            kind: "file".into(),
            extension: Some("txt".into()),
            mime: Some("text/plain".into()),
            ..Default::default()
        };
        assert!(InfoSubsectionMatch::default().matches(&metadata));
    }
}
