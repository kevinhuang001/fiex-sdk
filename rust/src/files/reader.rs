//! A `Read + Seek + Send + 'static` reader over a Core-owned file handle.
use crate::{context::ContextInner, runtime::transfer::ReadCache, RpcError};
use serde_json::json;
use std::io::{self, Read, Seek, SeekFrom};

pub struct FileReader {
    context: ContextInner,
    handle: String,
    offset: u64,
    length: u64,
    cache: ReadCache,
}

impl FileReader {
    pub(crate) fn new(context: ContextInner, handle: String, length: u64) -> Self {
        Self {
            context,
            handle,
            offset: 0,
            length,
            cache: ReadCache::default(),
        }
    }

    pub fn len(&self) -> u64 {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

impl Read for FileReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        self.cache
            .read(&self.context, &self.handle, &mut self.offset, output)
    }
}

impl Seek for FileReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let value = match position {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::Current(offset) => self.offset as i128 + offset as i128,
            SeekFrom::End(offset) => self.length as i128 + offset as i128,
        };
        if !(0..=u64::MAX as i128).contains(&value) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"));
        }
        self.offset = value as u64;
        Ok(self.offset)
    }
}

impl Drop for FileReader {
    fn drop(&mut self) {
        self.cache.release(&self.context);
        if !self.context.closed() {
            let _: Result<serde_json::Value, RpcError> = self
                .context
                .call("host.fs.closeRead", json!({"handle": self.handle}));
        }
    }
}
