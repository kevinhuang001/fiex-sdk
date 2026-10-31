use super::*;
use crate::{RequestContext, RpcError};
pub use fiex_provider_contracts::{FsEntry, FsKind, FsListing, SearchCapabilities, SearchMode};
use serde_json::Value;

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
