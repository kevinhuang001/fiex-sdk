//! Bounded VFS listing page shared by the SDK and Core resolver.
//! The SDK's typed FsListing serializes to this same wire envelope.
//! Node metadata is validated separately by Core after scope projection.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A directory page. Search uses the shared SearchResults envelope instead.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VfsPage {
    pub entries: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

pub(crate) fn validate_cursor(cursor: Option<&str>) -> Result<(), String> {
    if let Some(value) = cursor {
        if value.is_empty() || value.len() > 2048 || value.chars().any(char::is_control) {
            return Err("invalid pagination cursor".into());
        }
    }
    Ok(())
}

impl VfsPage {
    pub fn validate_page(&self, limit: usize) -> Result<(), String> {
        if self.entries.len() > limit {
            return Err("provider exceeded requested VFS page limit".into());
        }
        validate_cursor(self.next_cursor.as_deref())?;
        if self.warnings.len() > 256 || self.warnings.iter().any(|item| item.len() > 4096) {
            return Err("invalid VFS page warnings".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn vfs_pages_reject_malformed_control_flags_and_unbounded_cursors() {
        for bad in [
            json!({"entries": [], "truncated": null}),
            json!({"entries": [], "truncated": "false"}),
            json!({"entries": [], "nextCursor": 42}),
            json!({"entries": [], "warnings": "warning"}),
        ] {
            assert!(serde_json::from_value::<VfsPage>(bad).is_err());
        }
        let empty: VfsPage = serde_json::from_value(json!({"entries": []})).unwrap();
        assert!(!empty.truncated);
        assert_eq!(empty.next_cursor, None);
        assert!(empty.validate_page(0).is_ok());

        for cursor in ["", "bad\n", "x\t"] {
            let page = VfsPage {
                next_cursor: Some(cursor.into()),
                ..Default::default()
            };
            assert!(page.validate_page(500).is_err());
        }
        let long = VfsPage {
            next_cursor: Some("a".repeat(2049)),
            ..Default::default()
        };
        assert!(long.validate_page(500).is_err());
    }

    #[test]
    fn vfs_page_matches_sdk_listing_wire_shape() {
        let page: VfsPage = serde_json::from_value(json!({
            "entries": [{"nodeId": "a", "name": "file"}],
            "truncated": true,
            "nextCursor": "next",
            "warnings": ["slow mount"]
        }))
        .unwrap();
        assert!(page.validate_page(1).is_ok());
        assert!(page.validate_page(0).is_err());
        assert_eq!(page.next_cursor.as_deref(), Some("next"));
    }
}
