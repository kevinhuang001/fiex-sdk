//! Host-brokered file API. Plugins see bytes and ordinary `Read`/`Seek`/
//! `Write` adapters; Core owns the real handles and the authorization scope.
mod reader;
mod writer;

pub use reader::FileReader;
pub use writer::FileWriter;

use crate::{
    context::{rpc_to_io, ContextInner},
    ipc::Entry,
};
use serde_json::json;
use std::io::{self, Read};

const MAX_READ_ALL: usize = 64 * 1024 * 1024;

/// A cloneable, `'static` view of the operation's file access. It holds an
/// internal host handle rather than borrowing a request context.
#[derive(Clone)]
pub struct Files {
    context: ContextInner,
}

impl Files {
    pub(crate) fn new(context: ContextInner) -> Self {
        Self { context }
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

    pub fn open_read(&self, path: &str) -> io::Result<FileReader> {
        let value = self
            .context
            .call("host.fs.openRead", json!({"path": path}))
            .map_err(rpc_to_io)?;
        let handle = value["handle"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing read handle"))?;
        let length = value["size"]
            .as_u64()
            .ok_or_else(|| io::Error::other("missing file size"))?;
        Ok(FileReader::new(self.context.clone(), handle.into(), length))
    }

    pub fn begin_write(&self, path: &str, create: bool) -> io::Result<FileWriter> {
        let handle = self
            .context
            .call(
                "host.fs.beginWrite",
                json!({"path": path, "create": create}),
            )
            .map_err(rpc_to_io)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| io::Error::other("missing write handle"))?;
        Ok(FileWriter::new(self.context.clone(), handle))
    }

    pub fn write(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut writer = self.begin_write(path, false)?;
        writer.write_all(bytes)?;
        writer.commit()
    }

    pub fn create(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut writer = self.begin_write(path, true)?;
        writer.write_all(bytes)?;
        writer.commit()
    }

    pub fn list(&self, path: &str) -> io::Result<Vec<Entry>> {
        let value = self
            .context
            .call("host.fs.list", json!({"path": path}))
            .map_err(rpc_to_io)?;
        serde_json::from_value(value)
            .map_err(|error| io::Error::other(format!("invalid directory listing: {error}")))
    }

    pub fn metadata(&self, path: &str) -> io::Result<Entry> {
        let value = self
            .context
            .call("host.fs.stat", json!({"path": path}))
            .map_err(rpc_to_io)?;
        serde_json::from_value(value)
            .map_err(|error| io::Error::other(format!("invalid file metadata: {error}")))
    }

    /// Resolve metadata for the current selection with a single host request.
    /// A denied or missing entry fails independently without exposing its data.
    pub fn stat_batch(&self, paths: &[&str]) -> io::Result<Vec<io::Result<Entry>>> {
        let value = self
            .context
            .call("host.fs.statBatch", json!({"paths": paths}))
            .map_err(rpc_to_io)?;
        let results = value["results"]
            .as_array()
            .filter(|results| results.len() == paths.len())
            .ok_or_else(|| io::Error::other("invalid stat batch response"))?;
        Ok(results
            .iter()
            .map(|result| {
                if let Some(metadata) = result.get("metadata") {
                    serde_json::from_value(metadata.clone()).map_err(|error| {
                        io::Error::new(io::ErrorKind::InvalidData, error)
                    })
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        result["error"].as_str().unwrap_or("metadata unavailable"),
                    ))
                }
            })
            .collect())
    }

    pub fn create_dir_all(&self, path: &str) -> io::Result<()> {
        self.context
            .call("host.fs.mkdir", json!({"path": path}))
            .map_err(rpc_to_io)?;
        Ok(())
    }

    pub fn remove(&self, path: &str) -> io::Result<()> {
        self.context
            .call("host.fs.remove", json!({"path": path}))
            .map_err(rpc_to_io)?;
        Ok(())
    }

    pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        self.context
            .call("host.fs.rename", json!({"from": from, "to": to}))
            .map_err(rpc_to_io)?;
        Ok(())
    }
}
