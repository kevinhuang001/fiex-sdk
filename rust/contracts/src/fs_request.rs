//! Logical VFS request and non-serializable stream products shared by Core
//! native providers and the SDK's existing stream server.
use crate::{FsEntry, FsListing, FsOperation, ProviderError, ProviderInvocation, SearchMode};
use serde_json::Value;
use std::io::{Read, Write};

#[derive(Clone)]
pub struct FsRequest<'a> {
    ctx: &'a dyn ProviderInvocation,
    op: FsOperation,
    params: Value,
}

impl<'a> FsRequest<'a> {
    pub fn new(ctx: &'a dyn ProviderInvocation, op: FsOperation) -> Self {
        Self {
            params: ctx.params().clone(),
            ctx,
            op,
        }
    }

    pub fn op(&self) -> FsOperation {
        self.op
    }

    pub fn ctx(&self) -> &'a dyn ProviderInvocation {
        self.ctx
    }

    pub fn params(&self) -> &Value {
        &self.params
    }

    pub fn node_id(&self) -> Option<&str> {
        self.params.get("nodeId").and_then(Value::as_str)
    }

    pub fn node_id_or_root(&self) -> &str {
        self.node_id().unwrap_or("")
    }

    pub fn parent_id(&self) -> Option<&str> {
        self.params.get("parentId").and_then(Value::as_str)
    }

    pub fn parent_id_or_root(&self) -> &str {
        self.parent_id().unwrap_or("")
    }

    pub fn name(&self) -> Option<&str> {
        self.params.get("name").and_then(Value::as_str)
    }

    pub fn if_revision(&self) -> Option<&str> {
        self.params.get("ifRevision").and_then(Value::as_str)
    }

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

    pub fn search_mode(&self) -> Result<SearchMode, ProviderError> {
        serde_json::from_value(
            self.params
                .get("mode")
                .cloned()
                .ok_or_else(|| ProviderError::invalid_params("search mode is required"))?,
        )
        .map_err(|_| ProviderError::invalid_params("search mode must be name or content"))
    }

    pub fn mount_id(&self) -> Option<&str> {
        self.params.get("mountId").and_then(Value::as_str)
    }

    pub fn connection(&self) -> Option<&Value> {
        self.params.get("connection")
    }

    pub fn contribution_id(&self) -> Option<&str> {
        self.params.get("contributionId").and_then(Value::as_str)
    }

    pub fn view_context(&self) -> Option<&Value> {
        self.params.get("viewContext")
    }
}

pub struct FsRead {
    pub reader: Box<dyn Read + Send>,
    pub size: Option<u64>,
}

impl FsRead {
    pub fn new(reader: impl Read + Send + 'static, size: Option<u64>) -> Self {
        Self {
            reader: Box::new(reader),
            size,
        }
    }

    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        let bytes = bytes.into();
        let size = bytes.len() as u64;
        Self::new(std::io::Cursor::new(bytes), Some(size))
    }
}

pub struct FsWrite {
    pub writer: Box<dyn Write + Send>,
    commit: Option<Box<dyn FnOnce() -> Result<FsEntry, ProviderError> + Send>>,
}

impl FsWrite {
    pub fn new(writer: impl Write + Send + 'static) -> Self {
        Self {
            writer: Box::new(writer),
            commit: None,
        }
    }

    pub fn with_commit(
        writer: impl Write + Send + 'static,
        finish: impl FnOnce() -> Result<FsEntry, ProviderError> + Send + 'static,
    ) -> Self {
        Self {
            writer: Box::new(writer),
            commit: Some(Box::new(finish)),
        }
    }

    pub fn commit(mut self) -> Result<Option<FsEntry>, ProviderError> {
        self.writer
            .flush()
            .map_err(|error| ProviderError::internal(error.to_string()))?;
        match self.commit.take() {
            Some(finish) => finish().map(Some),
            None => Ok(None),
        }
    }

    pub fn abort(self) {}
}

pub enum FsProduct {
    Entry(FsEntry),
    Listing(FsListing),
    Read(FsRead),
    Write(FsWrite),
    Done,
}
