//! One declarative eligibility contract for native and IPC contributions.
//!
//! Matching does not grant any IO capability. Core builds trusted location and
//! operation snapshots before using these predicates, then authorizes again
//! during provider invocation. Plugins cannot mint a Core-owned location.
use crate::filesystem::FsOperation;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocationMatch {
    #[serde(default)]
    pub types: Option<Vec<String>>,
    #[serde(default)]
    pub provider_ids: Option<Vec<String>>,
    #[serde(default)]
    pub mount_ids: Option<Vec<String>>,
}

impl LocationMatch {
    /// OR inside each list and AND across the declared dimensions.
    pub fn matches(&self, location: &Value) -> bool {
        let field_matches = |key: &str, accepted: &Option<Vec<String>>| {
            accepted.as_ref().is_none_or(|values| {
                location
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|actual| values.iter().any(|value| value == actual))
            })
        };
        location.is_object()
            && field_matches("type", &self.types)
            && field_matches("providerId", &self.provider_ids)
            && field_matches("mountId", &self.mount_ids)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderMatchRule {
    #[serde(default)]
    pub view: Option<LocationMatch>,
    #[serde(default)]
    pub storage: Option<LocationMatch>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub mime_types: Vec<String>,
    /// File/Directory/Connection kind; not FS/VFS resource location type.
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub required_ops: Vec<FsOperation>,
    #[serde(default)]
    pub selection: Option<String>,
}

impl ProviderMatchRule {
    pub fn is_empty(&self) -> bool {
        self.view.is_none()
            && self.storage.is_none()
            && self.extensions.is_empty()
            && self.mime_types.is_empty()
            && self.kinds.is_empty()
            && self.required_ops.is_empty()
            && self.selection.is_none()
    }

    pub fn matches_selection(&self, count: usize) -> bool {
        match self.selection.as_deref().unwrap_or("any") {
            "any" => true,
            "zero" => count == 0,
            "single" => count == 1,
            "multi" => count >= 2,
            _ => false,
        }
    }

    pub fn matches(&self, ext: &str, mime: &str, is_dir: bool) -> bool {
        let kind = if is_dir { "directory" } else { "file" };
        self.matches_content(ext, mime)
            && (self.kinds.is_empty() || self.kinds.iter().any(|name| name == kind))
    }

    fn matches_content(&self, ext: &str, mime: &str) -> bool {
        let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
        let mime = mime.trim().to_ascii_lowercase();
        let ext_ok = self.extensions.is_empty()
            || self.extensions.iter().any(|v| {
                let v = v
                    .trim()
                    .trim_start_matches("*.")
                    .trim_start_matches('.')
                    .to_ascii_lowercase();
                v == "*" || v == ext
            });
        let mime_ok = self.mime_types.is_empty()
            || self.mime_types.iter().any(|pattern| {
                let pattern = pattern.trim().to_ascii_lowercase();
                if pattern == "*/*" || pattern == "*" {
                    return true;
                }
                match pattern.strip_suffix("/*") {
                    Some(prefix) => mime.split('/').next().is_some_and(|v| v == prefix),
                    None => pattern == mime,
                }
            });
        ext_ok && mime_ok
    }

    /// The caller decides which directory snapshot to use for a blank canvas.
    /// Selection cardinality always uses the original number of selected items.
    pub fn matches_targets(&self, targets: &[Value], selection_count: usize) -> bool {
        if !self.matches_selection(selection_count) {
            return false;
        }
        if targets.is_empty() {
            return self.extensions.is_empty()
                && self.mime_types.is_empty()
                && self.kinds.is_empty()
                && self.view.is_none()
                && self.storage.is_none()
                && self.required_ops.is_empty();
        }
        targets.iter().all(|m| {
            let kind = match m["kind"].as_str() {
                Some("directory") => Some("directory"),
                Some("file") => Some("file"),
                Some("connection") => Some("connection"),
                _ => m["isDir"].as_bool().map(|dir| if dir { "directory" } else { "file" }),
            };
            m.is_object()
                && (self.extensions.is_empty() || m["extension"].is_string())
                && (self.mime_types.is_empty() || m["mime"].is_string())
                && (self.kinds.is_empty()
                    || kind.is_some_and(|kind| self.kinds.iter().any(|name| name == kind)))
                && self.matches_content(
                    m["extension"].as_str().unwrap_or(""),
                    m["mime"].as_str().unwrap_or(""),
                )
                && self
                    .view
                    .as_ref()
                    .is_none_or(|rule| rule.matches(&m["view"]))
                && self
                    .storage
                    .as_ref()
                    .is_none_or(|rule| rule.matches(&m["storage"]))
                && self.required_ops.iter().all(|op| {
                    m["operations"].as_array().is_some_and(|ops| {
                        ops.iter().any(|value| value.as_str() == Some(op.as_str()))
                    })
                })
        })
    }

    /// Canonical manifest match validation shared by all extension points.
    pub fn validate(value: Option<&Value>, allow_selection: bool) -> Result<(), String> {
        let Some(value) = value else { return Ok(()) };
        let object = value.as_object().ok_or("match must be an object")?;
        if object.contains_key("contains") {
            return Err("Feature matching cannot depend on a host directory".into());
        }
        if !allow_selection && object.contains_key("selection") {
            return Err("match.selection is only valid for contextMenus".into());
        }
        for key in ["extensions", "mimeTypes", "kinds"] {
            if let Some(items) = object.get(key) {
                let items = items
                    .as_array()
                    .ok_or_else(|| format!("match.{key} must be an array"))?;
                if items.iter().any(|v| !v.is_string()) {
                    return Err(format!("match.{key} entries must be strings"));
                }
            }
        }
        for (label, target) in [
            ("view", object.get("view")),
            ("storage", object.get("storage")),
        ] {
            let Some(target) = target else { continue };
            let fields = target
                .as_object()
                .ok_or_else(|| format!("match.{label} must be an object"))?;
            if fields.is_empty() {
                return Err(format!(
                    "match.{label} must constrain types, providerIds or mountIds"
                ));
            }
            for (key, values) in fields {
                if !matches!(key.as_str(), "types" | "providerIds" | "mountIds") {
                    return Err(format!("match.{label}.{key} is unknown"));
                }
                let values = values
                    .as_array()
                    .ok_or_else(|| format!("match.{label}.{key} must be an array"))?;
                if values.is_empty() || values.len() > 64 {
                    return Err(format!("match.{label}.{key} must have 1..64 entries"));
                }
                for value in values {
                    let item = value
                        .as_str()
                        .ok_or_else(|| format!("match.{label}.{key} requires strings"))?;
                    if item.is_empty() || item.len() > 256 || item.chars().any(char::is_control) {
                        return Err(format!("match.{label}.{key} contains an invalid value"));
                    }
                    if key == "types" && !matches!(item, "fs" | "vfs") {
                        return Err(format!("match.{label}.types accepts only fs or vfs"));
                    }
                }
            }
        }
        if let Some(kinds) = object.get("kinds").and_then(Value::as_array) {
            if kinds
                .iter()
                .any(|value| !matches!(value.as_str(), Some("file" | "directory" | "connection")))
            {
                return Err("match.kinds accepts only file, directory or connection".into());
            }
        }
        if let Some(selection) = object.get("selection") {
            let choice = selection
                .as_str()
                .ok_or("match.selection must be any, zero, single or multi")?;
            if !matches!(choice, "any" | "zero" | "single" | "multi") {
                return Err("match.selection must be any, zero, single or multi".into());
            }
        }
        serde_json::from_value::<Self>(value.clone()).map_err(|err| err.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn location_matches_are_conjunctive_and_must_be_present() {
        let rule: ProviderMatchRule = serde_json::from_value(json!({
            "view":{"providerIds":["core:recent"]},
            "storage":{"types":["vfs"],"mountIds":["account-a"]},
            "kinds":["file"],"requiredOps":["read"]
        }))
        .unwrap();
        let entry = json!({
            "view":{"type":"vfs","providerId":"core:recent","mountId":"core:recent"},
            "storage":{"type":"vfs","providerId":"plugin:drive","mountId":"account-a"},
            "kind":"file","operations":["read"]
        });
        assert!(rule.matches_targets(std::slice::from_ref(&entry), 1));
        assert!(!rule.matches_targets(&[json!({"view":entry["view"],"kind":"file"})], 1));
        assert!(!rule.matches_targets(&[entry.clone(), json!({"kind":"file"})], 2));
    }

    #[test]
    fn connections_use_the_same_view_and_storage_constraints() {
        let rule: ProviderMatchRule = serde_json::from_value(json!({
            "kinds":["connection"],
            "view":{"types":["vfs"],"providerIds":["plugin:drive"]},
            "storage":{"mountIds":["connection-a"]}
        })).unwrap();
        let connection = json!({
            "kind":"connection",
            "view":{"type":"vfs","providerId":"plugin:drive","mountId":"connection-a"},
            "storage":{"type":"vfs","providerId":"plugin:drive","mountId":"connection-a"}
        });
        assert!(rule.matches_targets(&[connection.clone()], 1));
        assert!(!rule.matches_targets(&[json!({"kind":"file", "view":connection["view"], "storage":connection["storage"]})], 1));
        assert!(!rule.matches_targets(&[json!({"kind":"connection", "view":connection["view"]})], 1));
    }

    #[test]
    fn rejects_invalid_resource_types_and_empty_selectors() {
        for value in [
            json!({"view":{"types":["remote"]}}),
            json!({"view":{"types":[]}}),
            json!({"view":{"providerIds":[]}}),
            json!({"kinds":["vfs"]}),
            json!({"storage":{"domains":["drive"]}}),
        ] {
            assert!(
                ProviderMatchRule::validate(Some(&value), true).is_err(),
                "{value}"
            );
        }
    }
}
