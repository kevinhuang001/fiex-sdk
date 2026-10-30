//! A `Write + Seek + Send + 'static` transactional writer. Core keeps the data
//! private until `commit`; dropping an uncommitted writer aborts it.
use crate::{
    context::{rpc_to_io, ContextInner},
    ipc,
    runtime::transfer::{flush_pending, release_write_buffer, CHUNK},
    RpcError,
};
use serde_json::json;
use std::io::{self, Seek, SeekFrom, Write};

pub struct FileWriter {
    context: ContextInner,
    handle: String,
    finished: bool,
    pending: Vec<u8>,
    failed: bool,
    shared: Option<(ipc::SharedBuffer, ipc::BufferDescriptor)>,
}

impl FileWriter {
    pub(crate) fn new(context: ContextInner, handle: String) -> Self {
        Self {
            context,
            handle,
            finished: false,
            pending: Vec::with_capacity(CHUNK),
            failed: false,
            shared: None,
        }
    }

    /// Publish the staged contents. On failure the transaction remains aborted.
    pub fn commit(mut self) -> io::Result<()> {
        self.flush()?;
        self.context
            .call("fs.commit", json!({"handle": self.handle}))
            .map_err(rpc_to_io)?;
        self.finished = true;
        Ok(())
    }
}

impl Write for FileWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("transaction failed"));
        }
        if self.pending.len() == CHUNK {
            self.flush()?;
        }
        let size = bytes.len().min(CHUNK - self.pending.len());
        self.pending.extend_from_slice(&bytes[..size]);
        Ok(size)
    }

    fn flush(&mut self) -> io::Result<()> {
        flush_pending(
            &self.context,
            &self.handle,
            &mut self.pending,
            &mut self.failed,
            &mut self.shared,
        )
    }
}

impl Seek for FileWriter {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.flush()?;
        let (kind, offset) = match position {
            SeekFrom::Start(offset) => ("start", offset as i128),
            SeekFrom::Current(offset) => ("current", offset as i128),
            SeekFrom::End(offset) => ("end", offset as i128),
        };
        if offset < i64::MIN as i128 || offset > i64::MAX as i128 {
            return Err(io::Error::other("write seek out of range"));
        }
        self.context
            .call(
                "fs.seekWrite",
                json!({"handle": self.handle, "kind": kind, "offset": offset as i64}),
            )
            .map_err(rpc_to_io)?
            .as_u64()
            .ok_or_else(|| io::Error::other("invalid write offset"))
    }
}

impl Drop for FileWriter {
    fn drop(&mut self) {
        release_write_buffer(&self.context, &mut self.shared);
        if !self.finished && !self.context.closed() {
            let _: Result<serde_json::Value, RpcError> = self
                .context
                .call("fs.abort", json!({"handle": self.handle}));
        }
    }
}
