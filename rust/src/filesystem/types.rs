use super::*;
use crate::{RequestContext, RpcError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// A parsed `vfs.*` request. Providers read only the fields their operation
/// needs; the raw params stay available for extensions.
#[derive(Clone)]
pub struct FsRequest<'a> {
    ctx: &'a RequestContext,
    op: FsOperation,
    params: Value,
}

impl<'a> FsRequest<'a> {
    pub fn new(ctx: &'a RequestContext, op: FsOperation) -> Self {
        Self {
            ctx,
            op,
            params: ctx.params().clone(),
        }
    }

    pub fn op(&self) -> FsOperation {
        self.op
    }

    pub fn ctx(&self) -> &'a RequestContext {
        self.ctx
    }

    pub fn params(&self) -> &Value {
        &self.params
    }

    /// The node the operation targets. The empty string is the mount root, so
    /// providers that join paths can use `node_id_or_root()`.
    pub fn node_id(&self) -> Option<&str> {
        self.params.get("nodeId").and_then(Value::as_str)
    }

    /// Target node, with the mount root as the default.
    pub fn node_id_or_root(&self) -> &str {
        self.node_id().unwrap_or("")
    }

    /// Parent directory for `mkdir` and for creating a new file. Empty is the
    /// mount root.
    pub fn parent_id(&self) -> Option<&str> {
        self.params.get("parentId").and_then(Value::as_str)
    }

    /// Parent directory, with the mount root as the default.
    pub fn parent_id_or_root(&self) -> &str {
        self.parent_id().unwrap_or("")
    }

    /// New name for `mkdir`/`rename`, or the leaf name of a new file.
    pub fn name(&self) -> Option<&str> {
        self.params.get("name").and_then(Value::as_str)
    }

    /// Optimistic-concurrency token the provider must compare before writing.
    pub fn if_revision(&self) -> Option<&str> {
        self.params.get("ifRevision").and_then(Value::as_str)
    }

    /// Expected content length of a new file, when Core knows it up front.
    pub fn size(&self) -> Option<u64> {
        self.params.get("size").and_then(Value::as_u64)
    }

    pub fn hidden(&self) -> bool {
        self.params
            .get("hidden")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn ignores(&self) -> Vec<String> {
        self.params
            .get("ignores")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn cursor(&self) -> Option<&str> {
        self.params.get("cursor").and_then(Value::as_str)
    }

    pub fn limit(&self) -> usize {
        self.params
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(500)
            .clamp(1, 5_000) as usize
    }

    pub fn query(&self) -> Option<&str> {
        self.params.get("query").and_then(Value::as_str)
    }

    pub fn search_mode(&self) -> Result<SearchMode, RpcError> {
        serde_json::from_value(
            self.params
                .get("mode")
                .cloned()
                .ok_or_else(|| RpcError::invalid_params("search mode is required"))?,
        )
        .map_err(|_| RpcError::invalid_params("search mode must be name or content"))
    }

    /// Mount identity, when Core assigned one.
    pub fn mount_id(&self) -> Option<&str> {
        self.params.get("mountId").and_then(Value::as_str)
    }

    /// Connection configuration, for connection-backed mounts.
    pub fn connection(&self) -> Option<&Value> {
        self.params.get("connection")
    }

    /// Contribution identity of a standalone view.
    pub fn contribution_id(&self) -> Option<&str> {
        self.params.get("contributionId").and_then(Value::as_str)
    }

    /// Opaque view context of a standalone view.
    pub fn view_context(&self) -> Option<&Value> {
        self.params.get("viewContext")
    }
}
