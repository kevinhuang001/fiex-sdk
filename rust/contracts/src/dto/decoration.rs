//! Shared DecorationProvider response DTOs.
//!
//! Providers identify targets with Core-issued resource references. Core
//! resolves those references to presentation paths and strips Core-only
//! fields from untrusted plugin contributions.
use crate::ResourceRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecorationBadge {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub tooltip: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecorationStyle {
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub font_weight: Option<String>,
    #[serde(default)]
    pub font_style: Option<String>,
    #[serde(default)]
    pub background_light: Option<String>,
    #[serde(default)]
    pub background_dark: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecorationField {
    #[serde(default)]
    pub badges: Vec<DecorationBadge>,
    #[serde(default)]
    pub style: DecorationStyle,
}

/// Core-owned presentation metadata. These fields exist only for in-process
/// native providers and are omitted from the plugin wire format.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeDecorationBadge {
    pub text: String,
    pub color: String,
    pub tooltip: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub dot: bool,
}

/// One contribution from a DecorationProvider. The `ref` is the opaque target
/// supplied by Core in the invocation; provider-controlled paths and names do
/// not select which resource receives the decoration.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecorationContribution {
    #[serde(rename = "ref")]
    pub target: ResourceRef,
    #[serde(skip)]
    pub native_badges: Vec<NativeDecorationBadge>,
    #[serde(default)]
    pub badges: Vec<DecorationBadge>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub style: DecorationStyle,
    /// Per-column decorations keyed by `name`, `modified`, `size`, `kind` or
    /// `row`; Core validates the row-only style rule before rendering.
    #[serde(default)]
    pub fields: HashMap<String, DecorationField>,
    /// Namespaced `fiex-resource://` icon reference.
    #[serde(default)]
    pub icon: Option<String>,
    /// Core-only resolved path. Never serialized to or accepted from plugins.
    #[serde(skip)]
    pub icon_path: Option<String>,
}

impl DecorationContribution {
    pub fn new(target: ResourceRef) -> Self {
        Self {
            target,
            native_badges: Vec::new(),
            badges: Vec::new(),
            label: None,
            description: None,
            style: DecorationStyle::default(),
            fields: HashMap::new(),
            icon: None,
            icon_path: None,
        }
    }
}
