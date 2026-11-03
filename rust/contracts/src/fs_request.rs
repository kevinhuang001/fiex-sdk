//! Logical VFS request and non-serializable stream products shared by Core
//! native providers and the SDK's existing stream server.
use crate::{
    FsEntry, FsListing, FsOperation, ProviderError, ProviderInput, ProviderInvocation, SearchMode,
    VfsRequest,
};
use std::io::{Read, Write};

#[derive(Clone)]
pub struct FsRequest<'a> {
    ctx: &'a dyn ProviderInvocation,
    op: FsOperation,
    request: VfsRequest,
}

impl<'a> FsRequest<'a> {
    pub fn new(ctx: &'a dyn ProviderInvocation, op: FsOperation) -> Result<Self, ProviderError> {
        let request = match ctx.input()? {
            ProviderInput::Vfs(request) => request.clone(),
            _ => return Err(ProviderError::invalid_params("missing typed VFS request")),
        };
        Ok(Self { ctx, op, request })
    }

    pub fn op(&self) -> FsOperation {
        self.op
    }

    /// Serialize the typed request only at the VFS transport boundary.
    pub fn to_params(&self) -> Result<serde_json::Value, ProviderError> {
        serde_json::to_value(&self.request)
            .map_err(|error| ProviderError::internal(error.to_string()))
    }

    pub fn ctx(&self) -> &'a dyn ProviderInvocation {
        self.ctx
    }

    pub fn node_id(&self) -> Option<&str> {
        self.request.node_id.as_deref()
    }

    pub fn node_id_or_root(&self) -> &str {
        self.node_id().unwrap_or("")
    }

    pub fn parent_id(&self) -> Option<&str> {
        self.request.parent_id.as_deref()
    }

    pub fn parent_id_or_root(&self) -> &str {
        self.parent_id().unwrap_or("")
    }

    pub fn name(&self) -> Option<&str> {
        self.request.name.as_deref()
    }

    pub fn if_revision(&self) -> Option<&str> {
        self.request.if_revision.as_deref()
    }

    pub fn size(&self) -> Option<u64> {
        self.request.size
    }

    pub fn hidden(&self) -> bool {
        self.request.hidden
    }

    pub fn ignores(&self) -> Vec<String> {
        self.request.ignores.clone()
    }

    pub fn cursor(&self) -> Option<&str> {
        self.request.cursor.as_deref()
    }

    pub fn limit(&self) -> usize {
        self.request.bounded_limit(5_000)
    }

    pub fn query(&self) -> Option<&str> {
        self.request.query.as_deref()
    }

    pub fn search_mode(&self) -> Result<SearchMode, ProviderError> {
        self.request
            .mode
            .ok_or_else(|| ProviderError::invalid_params("search mode is required"))
    }

    pub fn mount_id(&self) -> Option<&str> {
        self.request.mount_id.as_deref()
    }

    pub fn connection(&self) -> Option<&crate::ConnectionSettings> {
        self.request.connection.as_ref()
    }

    pub fn connection_value(&self) -> Result<Option<serde_json::Value>, ProviderError> {
        self.request
            .connection_value()
            .map_err(ProviderError::invalid_params)
    }

    pub fn contribution_id(&self) -> Option<&str> {
        self.request.contribution_id.as_deref()
    }

    pub fn view_context(&self) -> Option<&std::collections::BTreeMap<String, serde_json::Value>> {
        self.request.view_context.as_ref()
    }
}

/// A bounded provider-owned random-read callback for a VFS stream.
pub type FsReadAt = Box<dyn FnMut(u64, usize) -> Result<(Vec<u8>, bool), ProviderError> + Send>;

pub struct FsRead {
    pub reader: Box<dyn Read + Send>,
    pub size: Option<u64>,
    pub read_at: Option<FsReadAt>,
}

impl FsRead {
    pub fn new(reader: impl Read + Send + 'static, size: Option<u64>) -> Self {
        Self {
            reader: Box::new(reader),
            size,
            read_at: None,
        }
    }

    /// Create a stream with provider-native bounded random reads. Providers
    /// that only expose sequential reads should use [`FsRead::new`].
    pub fn with_read_at(
        reader: impl Read + Send + 'static,
        size: Option<u64>,
        read_at: impl FnMut(u64, usize) -> Result<(Vec<u8>, bool), ProviderError> + Send + 'static,
    ) -> Self {
        Self {
            reader: Box::new(reader),
            size,
            read_at: Some(Box::new(read_at)),
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
