//! Shared-memory data plane for large payloads.
//!
//! The control channel remains JSON-RPC. Only bulk file contents, command
//! output and network bodies use these buffers.
//!
//! The first implementation uses named shared memory for portability. The
//! descriptor is deliberately opaque to callers; a later sandboxed transport
//! can replace `name` with an inherited file descriptor or Windows HANDLE
//! without changing the plugin-facing API.
use serde::{Deserialize, Serialize};
use shared_memory::{Shmem, ShmemConf};
use std::io::{self, Read};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferDescriptor {
    pub id: String,
    pub name: String,
    pub size: usize,
    pub writable: bool,
}

pub struct SharedBuffer {
    shmem: Shmem,
    descriptor: BufferDescriptor,
}

// The mapping is owned by the buffer registry; access is serialized by the
// host. Moving the wrapper between threads does not move the mapping.
unsafe impl Send for SharedBuffer {}

impl SharedBuffer {
    /// Allocate a shared buffer owned by the host or by a test harness.
    pub fn create(size: usize, writable: bool) -> Result<(Self, BufferDescriptor), String> {
        if size == 0 {
            return Err("shared buffer size must be non-zero".into());
        }
        let shmem = ShmemConf::new()
            .size(size)
            .create()
            .map_err(|error| error.to_string())?;
        let name = shmem.get_os_id().to_owned();
        let descriptor = BufferDescriptor {
            id: format!("buffer-{}", name.trim_start_matches('/')),
            name,
            size: shmem.len(),
            writable,
        };
        Ok((
            Self {
                shmem,
                descriptor: descriptor.clone(),
            },
            descriptor,
        ))
    }

    /// Map a buffer created by the other side of the connection.
    pub fn open(descriptor: &BufferDescriptor) -> Result<Self, String> {
        let shmem = ShmemConf::new()
            .size(descriptor.size)
            .os_id(descriptor.name.clone())
            .open()
            .map_err(|error| error.to_string())?;
        let size = shmem.len();
        Ok(Self {
            shmem,
            descriptor: BufferDescriptor {
                size,
                ..descriptor.clone()
            },
        })
    }

    pub fn descriptor(&self) -> &BufferDescriptor {
        &self.descriptor
    }

    pub fn size(&self) -> usize {
        self.descriptor.size
    }

    /// Host-only initialization; the public descriptor can stay read-only.
    pub fn fill(&mut self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > self.shmem.len() {
            return Err("shared buffer is too small".into());
        }
        // SAFETY: called by the owner before publishing the buffer.
        (unsafe { self.shmem.as_slice_mut() })[..bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    /// Host-side bounded transfer into an already allocated window.
    pub fn fill_from(&mut self, reader: &mut impl Read, limit: usize) -> io::Result<usize> {
        if limit > self.shmem.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "shared buffer too small",
            ));
        }
        // The host serializes transfers and publishes the valid length after read.
        reader.read(&mut unsafe { self.shmem.as_slice_mut() }[..limit])
    }

    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: the mapping lives for the lifetime of `self`. The host
        // serializes access through the plugin request boundary.
        unsafe { self.shmem.as_slice() }
    }

    pub fn as_mut_slice(&mut self) -> Result<&mut [u8], String> {
        if !self.descriptor.writable {
            return Err("shared buffer is read-only".into());
        }
        // SAFETY: the mapping lives for the lifetime of `self`. Callers must
        // not hold this borrow across the IPC boundary.
        Ok(unsafe { self.shmem.as_slice_mut() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_write_and_reopen() {
        let (mut buffer, descriptor) = SharedBuffer::create(4096, true).unwrap();
        buffer.as_mut_slice().unwrap()[..4].copy_from_slice(b"fiex");
        let reopened = SharedBuffer::open(&descriptor).unwrap();
        assert_eq!(&reopened.as_slice()[..4], b"fiex");
    }

    #[test]
    fn read_only_buffers_reject_writes() {
        let (_buffer, descriptor) = SharedBuffer::create(16, false).unwrap();
        let mut buffer = SharedBuffer::open(&descriptor).unwrap();
        assert!(buffer.as_mut_slice().is_err());
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
    pub extension: String,
    pub mime: String,
    pub readonly: bool,
    pub symlink: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessResult {
    pub code: Option<i32>,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub stdout_buffer: Option<BufferDescriptor>,
    #[serde(default)]
    pub stdout_size: Option<usize>,
    #[serde(default)]
    pub stdout_base64: Option<String>,
    #[serde(default)]
    pub stderr_base64: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub body_buffer: Option<BufferDescriptor>,
    #[serde(default)]
    pub body_size: usize,
    #[serde(default)]
    pub body_base64: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TcpStreamHandle {
    pub stream: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TcpRead {
    pub buffer: Option<BufferDescriptor>,
    pub size: usize,
    pub eof: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChunk {
    pub base64: String,
    pub size: usize,
    pub eof: bool,
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    #[test]
    fn host_camel_case_fields_are_preserved() {
        let response: HttpResponse =
            serde_json::from_str(r#"{"status":200,"bodySize":3,"bodyBase64":"AP+A"}"#).unwrap();
        assert_eq!(response.body_size, 3);
        assert_eq!(response.body_base64.as_deref(), Some("AP+A"));
        let process: ProcessResult =
            serde_json::from_str(r#"{"code":0,"stdoutSize":8192}"#).unwrap();
        assert_eq!(process.stdout_size, Some(8192));
    }
    #[test]
    fn owner_can_initialize_read_only_buffer() {
        let (mut buffer, descriptor) = SharedBuffer::create(4, false).unwrap();
        buffer.fill(b"test").unwrap();
        assert_eq!(buffer.as_slice(), b"test");
        assert!(!descriptor.writable);
        assert!(buffer.as_mut_slice().is_err());
    }
}

impl ProcessResult {
    pub fn stdout_bytes(&self) -> Result<Vec<u8>, String> {
        use base64::Engine;
        match &self.stdout_base64 {
            Some(value) => base64::engine::general_purpose::STANDARD
                .decode(value)
                .map_err(|e| e.to_string()),
            None => Ok(self.stdout.as_bytes().to_vec()),
        }
    }
}
