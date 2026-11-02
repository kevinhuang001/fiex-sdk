//! Typed local-search result contracts.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchTag {
    pub name: String,
    pub color: u8,
}

/// One result from the standard Local SearchProvider event.
///
/// VFS search has a different entry identity and uses [`crate::FsListing`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: u64,
    pub created: u64,
    pub extension: String,
    pub mime: String,
    pub link_target: Option<String>,
    #[serde(default)]
    pub allowed_ops: Vec<String>,
    pub tags: Vec<SearchTag>,
    #[serde(default)]
    pub has_cache: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub uploaded: bool,
    pub sync: Option<String>,
}

/// A bounded page returned by the standard Local SearchProvider event.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchResults {
    #[serde(default)]
    pub entries: Vec<SearchEntry>,
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
        crate::dto::pagination::validate_cursor(self.next_cursor.as_deref())
            .map_err(|_| "invalid search pagination cursor".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_page_rejects_wrong_flag_and_excessive_cursor() {
        let page = SearchResults {
            entries: vec![SearchEntry {
                path: "file".into(),
                name: "file".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(page.validate_page(1).is_ok());
        assert!(page.validate_page(0).is_err());
        let serialized = serde_json::to_value(&page).unwrap();
        assert!(serialized.get("truncated").is_none());
        assert!(serialized.get("nextCursor").is_none());
        assert!(serde_json::from_value::<SearchResults>(
            serde_json::json!({"entries":[], "truncated":"false"})
        )
        .is_err());
        assert!(serde_json::from_value::<SearchResults>(
            serde_json::json!({"entries":[], "truncated":null})
        )
        .is_err());
        let malformed = SearchResults {
            next_cursor: Some("x".repeat(2049)),
            ..Default::default()
        };
        assert!(malformed.validate_page(1).is_err());
    }
}
