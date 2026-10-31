//! Shared, runtime-independent provider contracts for both Fiex Core
//! and resident plugin SDK implementations. No IPC, Tauri, HostLink or
//! platform dependencies belong in this crate.
use serde::{Deserialize, Serialize};

/// One capability a file-system provider can implement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FsOperation {
    List,
    Stat,
    Read,
    Write,
    Mkdir,
    Rename,
    Remove,
    Search,
}


impl FsOperation {
    pub const ALL: [FsOperation; 8] = [
        FsOperation::List,
        FsOperation::Stat,
        FsOperation::Read,
        FsOperation::Write,
        FsOperation::Mkdir,
        FsOperation::Rename,
        FsOperation::Remove,
        FsOperation::Search,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FsOperation::List => "list",
            FsOperation::Stat => "stat",
            FsOperation::Read => "read",
            FsOperation::Write => "write",
            FsOperation::Mkdir => "mkdir",
            FsOperation::Rename => "rename",
            FsOperation::Remove => "remove",
            FsOperation::Search => "search",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        FsOperation::ALL.into_iter().find(|op| op.as_str() == value)
    }

    /// Resolve a storage-provider wire method to the shared logical operation.
    pub fn from_method(method: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|op| op.methods().contains(&method))
    }

    /// Wire methods unlocked by this capability.
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            FsOperation::List => &["vfs.list"],
            FsOperation::Stat => &["vfs.stat"],
            FsOperation::Read => &["vfs.openRead", "vfs.readStream", "vfs.closeRead"],
            FsOperation::Write => &[
                "vfs.beginWrite",
                "vfs.writeStream",
                "vfs.commit",
                "vfs.abort",
            ],
            FsOperation::Mkdir => &["vfs.mkdir"],
            FsOperation::Rename => &["vfs.rename"],
            FsOperation::Remove => &["vfs.remove"],
            FsOperation::Search => &["vfs.search"],
        }
    }
}

impl Serialize for FsOperation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FsOperation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        FsOperation::parse(&value)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown fs operation: {value}")))
    }
}


mod endpoint;
pub use endpoint::{event, Dispatch, Event, ProviderEndpoint, ProviderRequest};
mod instances;
pub use instances::ProviderInstances;
mod pagination;
pub use pagination::VfsPage;
mod vfs;
pub use vfs::{FsEntry, FsKind, FsListing, SearchCapabilities, SearchMode};

mod resources;
pub use resources::{
    EventContext, InfoRow, InfoSection, PackageView, PreviewView,
    ResourceContext, ResourceMetadata, ResourceRef, SearchResults,
};
pub mod matching;
pub use matching::{LocationMatch, ProviderMatchRule};

/// Transport-independent provider kind. A VFS owns storage; the other
/// kinds contribute behavior and may match any Core-resolved view/storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Vfs, ContextMenu, ConnectionContextMenu, Info, Preview,
    Decoration, Search, Hook, Toolbar, Connection,
}

/// Identity and matching are data, not permissions. Core checks the manifest,
/// installation consent and resource scope separately on every invocation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDescriptor {
    pub id: String,
    pub kind: ProviderKind,
    pub priority: i64,
    #[serde(default)]
    pub r#match: ProviderMatchRule,
}
impl ProviderDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.id.len() > 128
            || !self.id.chars().all(|ch| ch.is_ascii_alphanumeric()
                || matches!(ch, '.' | '_' | '-' | ':'))
        {
            return Err("invalid provider ID".into());
        }
        if !(-100_000..=100_000).contains(&self.priority) {
            return Err("provider priority out of bounds".into());
        }
        if self.kind != ProviderKind::ContextMenu && self.r#match.selection.is_some() {
            return Err("match.selection is only valid for contextMenus".into());
        }
        self.r#match.validate_consistency()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn descriptors_reject_invalid_id_and_priority() {
        let good = ProviderDescriptor {
            id:"core.metadata".into(),kind:ProviderKind::Info, priority:100,
            r#match:ProviderMatchRule::default(),
        };
        assert!(good.validate().is_ok());
        let mut invalid = good.clone();
        invalid.id = "bad/id".into();
        assert!(invalid.validate().is_err());
        invalid = good;
        invalid.priority = i64::MAX;
        assert!(invalid.validate().is_err());

        let invalid = ProviderDescriptor {
            id: "core.illegal-selection".into(),
            kind: ProviderKind::Info,
            priority: 0,
            r#match: ProviderMatchRule {
                selection: Some("single".into()),
                ..Default::default()
            },
        };
        assert!(invalid.validate().is_err());
        let menu = ProviderDescriptor {
            kind: ProviderKind::ContextMenu,
            ..invalid
        };
        assert!(menu.validate().is_ok());
    }
}
