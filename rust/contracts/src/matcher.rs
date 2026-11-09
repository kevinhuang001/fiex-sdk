//! Shared matching evaluation for provider descriptor DTOs.
use crate::ResourceLocation;
use crate::dto::matching::{LocationMatch, MatchResource, ProviderMatchExclude, ProviderMatchRule};
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
        }) && self
            .provider_ids
            .as_ref()
            .is_none_or(|values| values.iter().any(|value| value == &location.provider_id))
            && self.mount_ids.as_ref().is_none_or(|values| {
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
            && self.exclude.is_none()
    }

    pub fn matches_selection(&self, count: usize) -> bool {
        match self.selection.as_deref().unwrap_or("any") {
            "any" => true,
            "zero" => count == 0,
            "single" => count == 1,
            "multi" => count >= 2,
            "nonzero" => count >= 1,
            _ => false,
        }
    }

    fn matches_content(&self, ext: Option<&str>, mime: Option<&str>) -> bool {
        matches_content_fields(&self.extensions, &self.mime_types, ext, mime)
    }

    /// The caller decides which directory snapshot to use for a blank canvas.
    /// Selection cardinality always uses the original number of selected items.
    pub fn matches_targets(&self, targets: &[MatchResource], selection_count: usize) -> bool {
        if !self.matches_selection(selection_count) {
            return false;
        }
        self.matches_included_targets(targets)
            && !self
                .exclude
                .as_ref()
                .is_some_and(|exclude| exclude.matches_any_target(targets))
    }

    fn matches_included_targets(&self, targets: &[MatchResource]) -> bool {
        if targets.is_empty() {
            return self.extensions.is_empty()
                && self.mime_types.is_empty()
                && self.kinds.is_empty()
                && self.view.is_none()
                && self.storage.is_none()
                && self.required_ops.is_empty();
        }
        targets.iter().all(|target| {
            let kind = target
                .metadata
                .as_ref()
                .map(|metadata| metadata.kind.as_str());
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
                && self
                    .view
                    .as_ref()
                    .is_none_or(|rule| rule.matches(target.view.as_ref()))
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
        validate_location_field("match", "view", object.get("view"))?;
        validate_location_field("match", "storage", object.get("storage"))?;
        if let Some(kinds) = object.get("kinds").and_then(Value::as_array)
            && kinds
                .iter()
                .any(|value| !matches!(value.as_str(), Some("file" | "directory" | "connection")))
        {
            return Err("match.kinds accepts only file, directory or connection".into());
        }
        if let Some(selection) = object.get("selection") {
            let choice = selection
                .as_str()
                .ok_or("match.selection must be any, zero, single, multi or nonzero")?;
            if !matches!(choice, "any" | "zero" | "single" | "multi" | "nonzero") {
                return Err("match.selection must be any, zero, single, multi or nonzero".into());
            }
        }
        if let Some(exclude) = object.get("exclude") {
            let fields = exclude
                .as_object()
                .ok_or("match.exclude must be an object")?;
            if fields.is_empty() {
                return Err("match.exclude must contain at least one condition".into());
            }
            if fields.contains_key("selection") || fields.contains_key("exclude") {
                return Err("match.exclude cannot contain selection or another exclude".into());
            }
            validate_exclude_fields(fields)?;
            serde_json::from_value::<ProviderMatchExclude>(exclude.clone())
                .map_err(|error| error.to_string())?;
        }
        serde_json::from_value::<Self>(value.clone()).map_err(|err| err.to_string())?;
        Ok(())
    }
}

fn matches_content_fields(
    extensions: &[String],
    mime_types: &[String],
    ext: Option<&str>,
    mime: Option<&str>,
) -> bool {
    let ext = ext.map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase());
    let mime = mime.map(|value| value.trim().to_ascii_lowercase());
    let ext_ok = extensions.is_empty()
        || ext.as_deref().is_some_and(|ext| {
            extensions.iter().any(|value| {
                let value = value
                    .trim()
                    .trim_start_matches("*.")
                    .trim_start_matches('.')
                    .to_ascii_lowercase();
                value == "*" || value.as_str() == ext
            })
        });
    let mime_ok = mime_types.is_empty()
        || mime.as_deref().is_some_and(|mime| {
            mime_types.iter().any(|pattern| {
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

fn validate_location_field(
    prefix: &str,
    label: &str,
    target: Option<&Value>,
) -> Result<(), String> {
    let Some(target) = target else { return Ok(()) };
    let fields = target
        .as_object()
        .ok_or_else(|| format!("{prefix}.{label} must be an object"))?;
    if fields.is_empty() {
        return Err(format!(
            "{prefix}.{label} must constrain types, providerIds or mountIds"
        ));
    }
    for (key, values) in fields {
        if !matches!(key.as_str(), "types" | "providerIds" | "mountIds") {
            return Err(format!("{prefix}.{label}.{key} is unknown"));
        }
        let values = values
            .as_array()
            .ok_or_else(|| format!("{prefix}.{label}.{key} must be an array"))?;
        if values.is_empty() || values.len() > 64 {
            return Err(format!("{prefix}.{label}.{key} must have 1..64 entries"));
        }
        for value in values {
            let item = value
                .as_str()
                .ok_or_else(|| format!("{prefix}.{label}.{key} requires strings"))?;
            if item.is_empty() || item.len() > 256 || item.chars().any(char::is_control) {
                return Err(format!("{prefix}.{label}.{key} contains an invalid value"));
            }
            if key == "types" && !matches!(item, "fs" | "vfs") {
                return Err(format!("{prefix}.{label}.types accepts only fs or vfs"));
            }
        }
    }
    Ok(())
}

fn validate_exclude_fields(fields: &serde_json::Map<String, Value>) -> Result<(), String> {
    for key in fields.keys() {
        if !matches!(
            key.as_str(),
            "view" | "storage" | "extensions" | "mimeTypes" | "kinds" | "requiredOps"
        ) {
            return Err(format!("match.exclude.{key} is unknown"));
        }
    }
    validate_location_field("match.exclude", "view", fields.get("view"))?;
    validate_location_field("match.exclude", "storage", fields.get("storage"))?;
    for key in ["extensions", "mimeTypes", "kinds", "requiredOps"] {
        if let Some(values) = fields.get(key) {
            let values = values
                .as_array()
                .ok_or_else(|| format!("match.exclude.{key} must be an array"))?;
            if values.is_empty() || values.len() > 64 {
                return Err(format!("match.exclude.{key} must have 1..64 entries"));
            }
            for value in values {
                if key == "requiredOps" {
                    let operation = value
                        .as_str()
                        .ok_or("match.exclude.requiredOps entries must be strings")?;
                    if crate::FsOperation::parse(operation).is_none() {
                        return Err(format!("unknown filesystem operation: {operation}"));
                    }
                } else if !value.is_string() {
                    return Err(format!("match.exclude.{key} entries must be strings"));
                }
            }
        }
    }
    if let Some(kinds) = fields.get("kinds").and_then(Value::as_array)
        && kinds
            .iter()
            .any(|value| !matches!(value.as_str(), Some("file" | "directory" | "connection")))
    {
        return Err("match.exclude.kinds accepts only file, directory or connection".into());
    }
    Ok(())
}

impl ProviderMatchExclude {
    fn matches_any_target(&self, targets: &[MatchResource]) -> bool {
        targets.iter().any(|target| {
            let kind = target
                .metadata
                .as_ref()
                .map(|metadata| metadata.kind.as_str());
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
                && matches_content_fields(&self.extensions, &self.mime_types, extension, mime)
                && self
                    .view
                    .as_ref()
                    .is_none_or(|rule| rule.matches(target.view.as_ref()))
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
