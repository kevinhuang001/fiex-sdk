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
