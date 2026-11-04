//! One declarative eligibility contract for native and IPC contributions.
//!
//! Matching does not grant any IO capability. Core builds trusted location and
//! operation snapshots before using these predicates, then authorizes again
//! during provider invocation. Plugins cannot mint a Core-owned location.
use crate::{FsOperation, ResourceCapabilities, ResourceLocation, ResourceMetadata};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocationMatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_ids: Option<Vec<String>>,
}

/// Trusted, typed inputs to the shared provider matcher. Core constructs these
/// from its live resource and capability registries before candidate discovery.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MatchResource {
    pub metadata: Option<ResourceMetadata>,
    pub view: Option<ResourceLocation>,
    pub storage: Option<ResourceLocation>,
    pub capabilities: ResourceCapabilities,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderMatchRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<LocationMatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<String>,
    /// Exclude a provider when any selected resource (or the blank-canvas
    /// directory context) satisfies every declared exclusion dimension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<ProviderMatchExclude>,
}

/// Negative match dimensions share the same trusted inputs as inclusion
/// rules. Lists are OR within a dimension; declared dimensions are ANDed.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderMatchExclude {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<LocationMatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<LocationMatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mime_types: Vec<String>,
    /// File/Directory/Connection kind; not FS/VFS resource location type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_ops: Vec<FsOperation>,
}
