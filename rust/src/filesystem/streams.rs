use super::*;
use crate::RpcError;
use std::io::{Read, Write};

/// A readable stream returned by [`VfsProvider::read`].
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

/// A writable stream returned by [`VfsProvider::write`].
///
/// The provider writes into a standard [`Write`]; the SDK feeds it chunks from
/// Core and only publishes the file when [`FsWrite::commit`] runs. Protocols
/// that publish through a server-side rename (SMB staging, Drive resumable
/// upload) do their final step in the commit callback.
pub struct FsWrite {
    pub writer: Box<dyn Write + Send>,
    commit: Option<Box<dyn FnOnce() -> Result<FsEntry, RpcError> + Send>>,
}

impl FsWrite {
    pub fn new(writer: impl Write + Send + 'static) -> Self {
        Self {
            writer: Box::new(writer),
            commit: None,
        }
    }

    /// Run `finish` after the last chunk was written and flushed; its entry
    /// becomes the `vfs.commit` result.
    pub fn with_commit(
        writer: impl Write + Send + 'static,
        finish: impl FnOnce() -> Result<FsEntry, RpcError> + Send + 'static,
    ) -> Self {
        Self {
            writer: Box::new(writer),
            commit: Some(Box::new(finish)),
        }
    }

    pub fn commit(mut self) -> Result<Option<FsEntry>, RpcError> {
        self.writer
            .flush()
            .map_err(|error| RpcError::internal(error.to_string()))?;
        match self.commit.take() {
            Some(finish) => finish().map(Some),
            None => Ok(None),
        }
    }

    pub fn abort(self) {}
}

/// Result of one logical file-system operation.
pub enum FsProduct {
    Entry(FsEntry),
    Listing(FsListing),
    Read(FsRead),
    Write(FsWrite),
    Done,
}
