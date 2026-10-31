//! Canonical VFS entry, listing and search capability DTOs.
//! Pure data: safe for both the Core process and resident plugin SDK.
use crate::FsOperation;
use serde::{Deserialize, Serialize};

/// Independent modes under the single `vfs.search` RPC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    Name,
    Content,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchCapabilities {
    #[serde(default)]
    pub name: bool,
    #[serde(default)]
    pub content: bool,
}
impl SearchCapabilities {
    pub fn supports(&self, mode: SearchMode) -> bool {
        match mode {
            SearchMode::Name => self.name,
            SearchMode::Content => self.content,
        }
    }
}

/// Kind of a directory entry. Symlinks are reported as their own kind so Core
/// never has to guess whether a target was followed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FsKind {
    #[default]
    File,
    Directory,
    Symlink,
}

/// Metadata for one node. `node_id` is opaque to Core and stable for the
/// lifetime of the mount; it is never a host path.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsEntry {
    pub node_id: String,
    pub name: String,
    #[serde(default)]
    pub kind: FsKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    /// Changes whenever the content changes; required for conditional writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    /// Operations this particular node allows, narrower than the provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_ops: Option<Vec<FsOperation>>,
    /// Set by `search` results: location relative to the searched root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
}

impl FsEntry {
    pub fn file(node_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            name: name.into(),
            kind: FsKind::File,
            ..Self::default()
        }
    }

    pub fn directory(node_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            name: name.into(),
            kind: FsKind::Directory,
            ..Self::default()
        }
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    pub fn readonly(mut self) -> Self {
        self.readonly = Some(true);
        self
    }

    pub fn allowing(mut self, ops: impl IntoIterator<Item = FsOperation>) -> Self {
        self.allowed_ops = Some(ops.into_iter().collect());
        self
    }

    pub fn is_dir(&self) -> bool {
        self.kind == FsKind::Directory
    }
}

/// One page of a directory listing or a search result.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsListing {
    #[serde(default)]
    pub entries: Vec<FsEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

impl FsListing {
    pub fn new(entries: impl IntoIterator<Item = FsEntry>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
            ..Self::default()
        }
    }
}
