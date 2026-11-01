//! Shared matching evaluation for provider descriptor DTOs.
use crate::dto::matching::{LocationMatch, MatchResource, ProviderMatchRule};
use crate::ResourceLocation;
use serde_json::Value;

impl LocationMatch {
    /// OR inside each list and AND across the declared dimensions.
    pub fn matches(&self, location: Option<&ResourceLocation>) -> bool {
        let Some(location) = location else {
            return false;
        };
        self.types.as_ref().is_none_or(|values| {
            values
                .iter()
                .any(|value| value.as_str() == location_type(location))
        }) && self.provider_ids.as_ref().is_none_or(|values| {
            values
                .iter()
                .any(|value| value == &location.provider_id)
        }) && self.mount_ids.as_ref().is_none_or(|values| {
            location
                .mount_id
                .as_ref()
                .is_some_and(|actual| values.contains(actual))
        })
    }
}

fn location_type(location: &ResourceLocation) -> &'static str {
    match location.resource_type {
        crate::ResourceType::Fs => "fs",
        crate::ResourceType::Vfs => "vfs",
    }
}

impl ProviderMatchRule {
    pub fn validate_consistency(&self) -> Result<(), String> {
        // Round-trip through the canonical manifest validator.
        let mut value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        if let Some(fields) = value.as_object_mut() {
            fields.retain(|_, entry| !entry.is_null());
        }
        Self::validate(Some(&value), true)
    }

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

    fn matches_content(&self, ext: Option<&str>, mime: Option<&str>) -> bool {
        let ext = ext.map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase());
        let mime = mime.map(|value| value.trim().to_ascii_lowercase());
        let ext_ok = self.extensions.is_empty()
            || ext.as_deref().is_some_and(|ext| {
                self.extensions.iter().any(|value| {
                    let value = value
                        .trim()
                        .trim_start_matches("*.")
                        .trim_start_matches('.')
                        .to_ascii_lowercase();
                    value == "*" || value.as_str() == ext
                })
            });
        let mime_ok = self.mime_types.is_empty()
            || mime.as_deref().is_some_and(|mime| {
                self.mime_types.iter().any(|pattern| {
                    let pattern = pattern.trim().to_ascii_lowercase();
                    if pattern == "*/*" || pattern == "*" {
                        return true;
                    }
                    match pattern.strip_suffix("/*") {
                        Some(prefix) => mime.split('/').next().is_some_and(|value| value == prefix),
                        None => pattern.as_str() == mime,
                    }
                })
            });
        ext_ok && mime_ok
    }

    /// The caller decides which directory snapshot to use for a blank canvas.
    /// Selection cardinality always uses the original number of selected items.
    pub fn matches_targets(&self, targets: &[MatchResource], selection_count: usize) -> bool {
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
        targets.iter().all(|target| {
            let kind = target.metadata.as_ref().map(|metadata| metadata.kind.as_str());
            let extension = target
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.extension.as_deref());
            let mime = target
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.mime.as_deref());
            (self.kinds.is_empty()
                || kind.is_some_and(|kind| self.kinds.iter().any(|name| name == kind)))
                && self.matches_content(extension, mime)
                && self.view.as_ref().is_none_or(|rule| rule.matches(target.view.as_ref()))
                && self
                    .storage
                    .as_ref()
                    .is_none_or(|rule| rule.matches(target.storage.as_ref()))
                && self
                    .required_ops
                    .iter()
                    .all(|operation| target.capabilities.allows(*operation))
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

/// Evaluates the canonical rule for both selected resources and blank-canvas
/// events. A zero-selection event is matched against its trusted directory
/// context, while its original cardinality remains zero for `selection`.
pub struct ProviderMatcher;

impl ProviderMatcher {
    pub fn matches(
        rule: &ProviderMatchRule,
        targets: &[MatchResource],
        directory: Option<&MatchResource>,
        selection_count: usize,
    ) -> bool {
        let context = if targets.is_empty() {
            directory.map(std::slice::from_ref).unwrap_or(&[])
        } else {
            targets
        };
        rule.matches_targets(context, selection_count)
    }
}


#[cfg(test)]
#[path = "matching_tests.rs"]
mod tests;
