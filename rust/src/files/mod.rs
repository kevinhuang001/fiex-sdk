//! Ref-only file service. Core resolves identities inside the captured operation
//! scope. File names are single child names, never host filesystem paths.
mod reader;
mod writer;
use crate::{
    context::{rpc_to_io, ContextInner},
    ResourceContext, ResourceMetadata, ResourceRef,
};
pub use reader::FileReader;
use serde_json::json;
use std::io::{self, Read};
pub use writer::FileWriter;
const MAX_READ_ALL: usize = 64 * 1024 * 1024;
#[derive(Clone)]
pub struct Files {
    context: ContextInner,
}
impl Files {
    pub(crate) fn new(context: ContextInner) -> Self {
        Self { context }
    }
    pub fn read(&self, reference: &ResourceRef) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.open_read(reference)?
            .take((MAX_READ_ALL + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_READ_ALL {
            return Err(io::Error::other(
                "file exceeds read-all limit; use open_read for streaming",
            ));
        }
        Ok(bytes)
    }
    pub fn read_to_string(&self, reference: &ResourceRef) -> io::Result<String> {
        String::from_utf8(self.read(reference)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
    pub fn open_read(&self, reference: &ResourceRef) -> io::Result<FileReader> {
        let value = self
            .context
            .call("host.fs.openRead", json!({"ref":reference}))
            .map_err(rpc_to_io)?;
        Ok(FileReader::new(
            self.context.clone(),
            value["handle"]
                .as_str()
                .ok_or_else(|| io::Error::other("missing read handle"))?
                .into(),
            value["size"]
                .as_u64()
                .ok_or_else(|| io::Error::other("missing file size"))?,
        ))
    }
    /// Reserve an identity below an authorized directory. This does not create
    /// a file or disclose metadata; creation needs its own manifest permission.
    pub fn child(&self, parent: &ResourceRef, name: &str) -> io::Result<ResourceRef> {
        let value = self
            .context
            .call("host.fs.child", json!({"ref":parent,"name":name}))
            .map_err(rpc_to_io)?;
        serde_json::from_value(value["ref"].clone()).map_err(io::Error::other)
    }
    pub fn begin_write(&self, reference: &ResourceRef, create: bool) -> io::Result<FileWriter> {
        let handle = self
            .context
            .call(
                "host.fs.beginWrite",
                json!({"ref":reference,"create":create}),
            )
            .map_err(rpc_to_io)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| io::Error::other("missing write handle"))?;
        Ok(FileWriter::new(self.context.clone(), handle))
    }
    pub fn write(&self, reference: &ResourceRef, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut writer = self.begin_write(reference, false)?;
        writer.write_all(bytes)?;
        writer.commit()
    }
    pub fn create(&self, reference: &ResourceRef, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut writer = self.begin_write(reference, true)?;
        writer.write_all(bytes)?;
        writer.commit()
    }
    pub fn list(&self, reference: &ResourceRef) -> io::Result<Vec<ResourceContext>> {
        serde_json::from_value(
            self.context
                .call("host.fs.list", json!({"ref":reference}))
                .map_err(rpc_to_io)?,
        )
        .map_err(io::Error::other)
    }
    /// Refresh metadata through Core; target.metadata is the event-time snapshot.
    pub fn metadata(&self, reference: &ResourceRef) -> io::Result<ResourceMetadata> {
        serde_json::from_value(
            self.context
                .call("host.fs.stat", json!({"ref":reference}))
                .map_err(rpc_to_io)?,
        )
        .map_err(io::Error::other)
    }
    pub fn stat_batch(
        &self,
        references: &[&ResourceRef],
    ) -> io::Result<Vec<io::Result<ResourceMetadata>>> {
        let value = self
            .context
            .call("host.fs.statBatch", json!({"refs":references}))
            .map_err(rpc_to_io)?;
        let results = value["results"]
            .as_array()
            .filter(|results| results.len() == references.len())
            .ok_or_else(|| io::Error::other("invalid stat batch response"))?;
        Ok(results
            .iter()
            .map(|value| {
                if let Some(metadata) = value.get("metadata") {
                    serde_json::from_value(metadata.clone()).map_err(io::Error::other)
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        value["error"].as_str().unwrap_or("metadata unavailable"),
                    ))
                }
            })
            .collect())
    }
    pub fn create_dir(&self, reference: &ResourceRef) -> io::Result<()> {
        self.context
            .call("host.fs.mkdir", json!({"ref":reference}))
            .map_err(rpc_to_io)?;
        Ok(())
    }
    pub fn remove(&self, reference: &ResourceRef) -> io::Result<()> {
        self.context
            .call("host.fs.remove", json!({"ref":reference}))
            .map_err(rpc_to_io)?;
        Ok(())
    }
    pub fn rename(&self, reference: &ResourceRef, destination: &ResourceRef) -> io::Result<()> {
        self.context
            .call(
                "host.fs.rename",
                json!({"ref":reference,"destination":destination}),
            )
            .map_err(rpc_to_io)?;
        Ok(())
    }
}
