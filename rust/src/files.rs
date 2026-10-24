//! High-level file APIs. The SDK owns framing and chunk encoding; plugins
//! receive bytes and ordinary Read/Seek/Write adapters, never local handles.
use crate::{HostServices, RpcError};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom, Write};

trait FileTransport: HostServices {
    fn fs_begin_write(&mut self, path: &str, create: bool) -> Result<String, RpcError> {
        self.invoke(
            "host.fs.beginWrite",
            json!({"path": path, "create": create}),
        )?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| RpcError::internal("missing write handle"))
    }
    fn fs_write_chunk(&mut self, handle: &str, base64: &str) -> Result<(), RpcError> {
        self.invoke(
            "host.fs.writeChunk",
            json!({"handle": handle, "base64": base64}),
        )?;
        Ok(())
    }
    fn fs_write_buffer(&mut self, handle: &str, buffer: &str, size: usize) -> Result<(), RpcError> {
        self.invoke(
            "host.fs.writeBuffer",
            json!({"handle":handle,"buffer":buffer,"size":size}),
        )?;
        Ok(())
    }
    fn fs_commit(&mut self, handle: &str) -> Result<Value, RpcError> {
        self.invoke("host.fs.commit", json!({"handle": handle}))
    }
    fn fs_abort(&mut self, handle: &str) -> Result<(), RpcError> {
        self.invoke("host.fs.abort", json!({"handle": handle}))?;
        Ok(())
    }

    fn fs_list(&mut self, path: &str) -> Result<Vec<crate::ipc::Entry>, RpcError> {
        let value = self.invoke("host.fs.list", json!({"path": path}))?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid directory listing: {error}")))
    }
    fn fs_stat(&mut self, path: &str) -> Result<crate::ipc::Entry, RpcError> {
        let value = self.invoke("host.fs.stat", json!({"path": path}))?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid file stat: {error}")))
    }
    fn fs_remove(&mut self, path: &str) -> Result<(), RpcError> {
        self.invoke("host.fs.remove", json!({"path": path}))?;
        Ok(())
    }
    fn fs_rename(&mut self, from: &str, to: &str) -> Result<(), RpcError> {
        self.invoke("host.fs.rename", json!({"from": from, "to": to}))?;
        Ok(())
    }
    fn fs_open_read(&mut self, path: &str) -> Result<(String, u64), RpcError> {
        let value = self.invoke("host.fs.openRead", json!({"path":path}))?;
        let handle = value["handle"]
            .as_str()
            .ok_or_else(|| RpcError::internal("missing read handle"))?;
        let size = value["size"]
            .as_u64()
            .ok_or_else(|| RpcError::internal("missing file size"))?;
        Ok((handle.into(), size))
    }
    fn fs_read_open(
        &mut self,
        handle: &str,
        offset: u64,
        length: usize,
        buffer: Option<&str>,
    ) -> Result<Value, RpcError> {
        self.invoke(
            "host.fs.readOpen",
            json!({"handle":handle,"offset":offset,"length":length,"buffer":buffer}),
        )
    }
    fn fs_close_read(&mut self, handle: &str) -> Result<(), RpcError> {
        self.invoke("host.fs.closeRead", json!({"handle":handle}))?;
        Ok(())
    }
}
impl<T: HostServices + ?Sized> FileTransport for T {}

const CHUNK: usize = 256 * 1024;
const SHARED_WINDOW: usize = 4 * 1024 * 1024;
const MAX_READ_ALL: usize = 64 * 1024 * 1024;
fn io_error(error: RpcError) -> io::Error {
    io::Error::other(error.to_string())
}

pub struct FileSystem<'a> {
    host: RefCell<&'a mut dyn HostServices>,
}
impl dyn HostServices + '_ {
    pub fn files(&mut self) -> FileSystem<'_> {
        FileSystem {
            host: RefCell::new(self),
        }
    }
}
impl<'a> FileSystem<'a> {
    pub fn list(&self, path: &str) -> io::Result<Vec<crate::ipc::Entry>> {
        self.host.borrow_mut().fs_list(path).map_err(io_error)
    }
    pub fn metadata(&self, path: &str) -> io::Result<crate::ipc::Entry> {
        self.host.borrow_mut().fs_stat(path).map_err(io_error)
    }
    pub fn create_dir_all(&self, path: &str) -> io::Result<()> {
        self.host
            .borrow_mut()
            .invoke("host.fs.mkdir", serde_json::json!({"path":path}))
            .map_err(io_error)?;
        Ok(())
    }
    pub fn remove(&self, path: &str) -> io::Result<()> {
        self.host.borrow_mut().fs_remove(path).map_err(io_error)
    }
    pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        self.host.borrow_mut().fs_rename(from, to).map_err(io_error)
    }
    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let mut data = Vec::new();
        self.open_read(path)?
            .take((MAX_READ_ALL + 1) as u64)
            .read_to_end(&mut data)?;
        if data.len() > MAX_READ_ALL {
            return Err(io::Error::other(
                "file exceeds read-all limit; use open_read for streaming",
            ));
        }
        Ok(data)
    }
    pub fn read_to_string(&self, path: &str) -> io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
    pub fn open_read(&self, path: &str) -> io::Result<FileReader<'_, 'a>> {
        let (handle, length) = self
            .host
            .borrow_mut()
            .fs_open_read(path)
            .map_err(io_error)?;
        Ok(FileReader {
            host: &self.host,
            handle,
            offset: 0,
            length,
            cache: ReadCache::default(),
        })
    }
    pub fn begin_write(&self, path: &str, create: bool) -> io::Result<FileWriter<'_, 'a>> {
        let handle = self
            .host
            .borrow_mut()
            .fs_begin_write(path, create)
            .map_err(io_error)?;
        Ok(FileWriter {
            host: &self.host,
            handle,
            finished: false,
            pending: Vec::with_capacity(CHUNK),
            failed: false,
            shared: None,
        })
    }
    pub fn write(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        let mut writer = self.begin_write(path, false)?;
        writer.write_all(bytes)?;
        writer.commit()
    }
    pub fn create(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        let mut writer = self.begin_write(path, true)?;
        writer.write_all(bytes)?;
        writer.commit()
    }
}

pub struct FileReader<'h, 'a> {
    host: &'h RefCell<&'a mut dyn HostServices>,
    handle: String,
    offset: u64,
    length: u64,
    cache: ReadCache,
}
impl Read for FileReader<'_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        self.cache.read(
            &mut **self.host.borrow_mut(),
            &self.handle,
            &mut self.offset,
            output,
        )
    }
}
impl Drop for FileReader<'_, '_> {
    fn drop(&mut self) {
        self.cache.release(&mut **self.host.borrow_mut());
        let _ = self.host.borrow_mut().fs_close_read(&self.handle);
    }
}
impl Seek for FileReader<'_, '_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let value = match position {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.offset as i128 + n as i128,
            SeekFrom::End(n) => self.length as i128 + n as i128,
        };
        if !(0..=u64::MAX as i128).contains(&value) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"));
        }
        self.offset = value as u64;
        Ok(self.offset)
    }
}

/// Writes remain private to Core until commit. Dropping a writer aborts it.
pub struct FileWriter<'h, 'a> {
    host: &'h RefCell<&'a mut dyn HostServices>,
    handle: String,
    finished: bool,
    pending: Vec<u8>,
    failed: bool,
    shared: Option<(crate::ipc::SharedBuffer, crate::ipc::BufferDescriptor)>,
}
impl FileWriter<'_, '_> {
    pub fn commit(mut self) -> io::Result<()> {
        self.flush()?;
        self.host
            .borrow_mut()
            .fs_commit(&self.handle)
            .map_err(io_error)?;
        self.finished = true;
        Ok(())
    }
}
impl Write for FileWriter<'_, '_> {
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
            &mut **self.host.borrow_mut(),
            &self.handle,
            &mut self.pending,
            &mut self.failed,
            &mut self.shared,
        )
    }
}
impl Seek for FileWriter<'_, '_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.flush()?;
        let (kind, offset) = match position {
            SeekFrom::Start(n) => ("start", n as i128),
            SeekFrom::Current(n) => ("current", n as i128),
            SeekFrom::End(n) => ("end", n as i128),
        };
        if offset < i64::MIN as i128 || offset > i64::MAX as i128 {
            return Err(io::Error::other("write seek out of range"));
        }
        self.host
            .borrow_mut()
            .invoke(
                "host.fs.seekWrite",
                serde_json::json!({"handle":self.handle,"kind":kind,"offset":offset as i64}),
            )
            .map_err(io_error)?
            .as_u64()
            .ok_or_else(|| io::Error::other("invalid write offset"))
    }
}

impl Drop for FileWriter<'_, '_> {
    fn drop(&mut self) {
        release_write_buffer(&mut **self.host.borrow_mut(), &mut self.shared);
        if !self.finished {
            let _ = self.host.borrow_mut().fs_abort(&self.handle);
        }
    }
}

/// Owned reader for libraries that retain Read + Seek + Send + 'static inputs.
/// It still performs all I/O through the serving thread's Host proxy.
pub struct OwnedFileReader {
    host: crate::worker::HostProxy,
    handle: String,
    offset: u64,
    length: u64,
    cache: ReadCache,
}
impl OwnedFileReader {
    pub fn open(mut host: crate::worker::HostProxy, path: &str) -> io::Result<Self> {
        let (handle, length) = host.fs_open_read(path).map_err(io_error)?;
        Ok(Self {
            host,
            handle,
            offset: 0,
            length,
            cache: ReadCache::default(),
        })
    }
}
impl Read for OwnedFileReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        self.cache
            .read(&mut self.host, &self.handle, &mut self.offset, output)
    }
}
impl Drop for OwnedFileReader {
    fn drop(&mut self) {
        self.cache.release(&mut self.host);
        let _ = self.host.fs_close_read(&self.handle);
    }
}
impl Seek for OwnedFileReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let value = match position {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.offset as i128 + n as i128,
            SeekFrom::End(n) => self.length as i128 + n as i128,
        };
        self.offset = u64::try_from(value).map_err(io::Error::other)?;
        Ok(self.offset)
    }
}

/// Owned transaction writer for libraries retaining a 'static Write sink.
pub struct OwnedFileWriter {
    host: crate::worker::HostProxy,
    handle: String,
    finished: bool,
    pending: Vec<u8>,
    failed: bool,
    shared: Option<(crate::ipc::SharedBuffer, crate::ipc::BufferDescriptor)>,
}
impl OwnedFileWriter {
    pub fn create(mut host: crate::worker::HostProxy, path: &str) -> io::Result<Self> {
        let handle = host.fs_begin_write(path, true).map_err(io_error)?;
        Ok(Self {
            host,
            handle,
            finished: false,
            pending: Vec::with_capacity(CHUNK),
            failed: false,
            shared: None,
        })
    }
    pub fn commit(mut self) -> io::Result<()> {
        self.flush()?;
        self.host.fs_commit(&self.handle).map_err(io_error)?;
        self.finished = true;
        Ok(())
    }
}
impl Write for OwnedFileWriter {
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
            &mut self.host,
            &self.handle,
            &mut self.pending,
            &mut self.failed,
            &mut self.shared,
        )
    }
}
impl Drop for OwnedFileWriter {
    fn drop(&mut self) {
        release_write_buffer(&mut self.host, &mut self.shared);
        if !self.finished {
            let _ = self.host.fs_abort(&self.handle);
        }
    }
}

// One bounded read-ahead window per reader. Seek within it requires no RPC.
#[derive(Default)]
struct ReadCache {
    start: u64,
    bytes: Vec<u8>,
    misses: usize,
    shared: Option<(crate::ipc::SharedBuffer, crate::ipc::BufferDescriptor)>,
}
impl ReadCache {
    fn release(&mut self, host: &mut dyn HostServices) {
        if let Some((_, descriptor)) = self.shared.take() {
            let _ = host.buffer_release(&descriptor.id);
        }
    }
    fn read(
        &mut self,
        host: &mut dyn HostServices,
        handle: &str,
        offset: &mut u64,
        output: &mut [u8],
    ) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let cached = offset
            .checked_sub(self.start)
            .and_then(|n| usize::try_from(n).ok());
        if !cached.is_some_and(|n| n < self.bytes.len()) {
            // Large sequential reads use one reusable shared window. Allocation or
            // mapping failure falls back to bounded IPC without changing the API.
            if (output.len() >= CHUNK || self.misses >= 2) && self.shared.is_none() {
                if let Ok(descriptor) = host.buffer_create(SHARED_WINDOW, false) {
                    match crate::ipc::SharedBuffer::open(&descriptor) {
                        Ok(mapped) => self.shared = Some((mapped, descriptor)),
                        Err(_) => {
                            let _ = host.buffer_release(&descriptor.id);
                        }
                    }
                }
            }
            let mut requested = if self.shared.is_some() {
                SHARED_WINDOW
            } else {
                CHUNK
            };
            let response = if let Some((_, descriptor)) = &self.shared {
                match host.fs_read_open(handle, *offset, requested, Some(&descriptor.id)) {
                    Ok(response) => response,
                    Err(_) => {
                        self.release(host);
                        requested = CHUNK;
                        host.fs_read_open(handle, *offset, requested, None)
                            .map_err(io_error)?
                    }
                }
            } else {
                host.fs_read_open(handle, *offset, requested, None)
                    .map_err(io_error)?
            };
            let size = response["size"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| io::Error::other("invalid file chunk size"))?;
            if size > requested {
                return Err(io::Error::other("invalid file chunk size"));
            }
            let bytes = if let Some((mapped, _)) = &self.shared {
                mapped.as_slice()[..size].to_vec()
            } else {
                let encoded = response["base64"]
                    .as_str()
                    .ok_or_else(|| io::Error::other("missing file chunk"))?;
                let bytes = STANDARD.decode(encoded).map_err(io::Error::other)?;
                if bytes.len() != size {
                    return Err(io::Error::other("invalid file chunk size"));
                }
                bytes
            };
            self.start = *offset;
            self.bytes = bytes;
            self.misses = self.misses.saturating_add(1);
        }
        let start = (*offset - self.start) as usize;
        let size = output.len().min(self.bytes.len() - start);
        output[..size].copy_from_slice(&self.bytes[start..start + size]);
        *offset = offset
            .checked_add(size as u64)
            .ok_or_else(|| io::Error::other("file offset overflow"))?;
        Ok(size)
    }
}
fn flush_pending(
    host: &mut dyn HostServices,
    handle: &str,
    pending: &mut Vec<u8>,
    failed: &mut bool,
    shared: &mut Option<(crate::ipc::SharedBuffer, crate::ipc::BufferDescriptor)>,
) -> io::Result<()> {
    if *failed {
        return Err(io::Error::other("transaction failed"));
    }
    if pending.is_empty() {
        return Ok(());
    }
    if pending.len() >= 64 * 1024 && shared.is_none() {
        if let Ok(descriptor) = host.buffer_create(CHUNK, true) {
            match crate::ipc::SharedBuffer::open(&descriptor) {
                Ok(mapped) => *shared = Some((mapped, descriptor)),
                Err(_) => {
                    let _ = host.buffer_release(&descriptor.id);
                }
            }
        }
    }
    let result = if let Some((mapped, descriptor)) = shared.as_mut() {
        match mapped.as_mut_slice() {
            Ok(bytes) => {
                bytes[..pending.len()].copy_from_slice(pending);
                host.fs_write_buffer(handle, &descriptor.id, pending.len())
            }
            Err(_) => {
                release_write_buffer(host, shared);
                host.fs_write_chunk(handle, &STANDARD.encode(&*pending))
            }
        }
    } else {
        host.fs_write_chunk(handle, &STANDARD.encode(&*pending))
    };
    if let Err(error) = result {
        *failed = true;
        return Err(io_error(error));
    }
    pending.clear();
    Ok(())
}

fn release_write_buffer(
    host: &mut dyn HostServices,
    shared: &mut Option<(crate::ipc::SharedBuffer, crate::ipc::BufferDescriptor)>,
) {
    if let Some((_, descriptor)) = shared.take() {
        let _ = host.buffer_release(&descriptor.id);
    }
}
