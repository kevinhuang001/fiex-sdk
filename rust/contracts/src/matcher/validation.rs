use serde_json::Value;

pub(super) fn validate_location_field(
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

pub(super) fn validate_exclude_fields(
    fields: &serde_json::Map<String, Value>,
) -> Result<(), String> {
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
