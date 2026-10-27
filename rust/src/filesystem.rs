//! Typed file-system providers.
//!
//! Core drives every file operation through one `fs.*` method family, no matter
//! whether the bytes live on a connection (SMB, Drive, WebDAV) or in a
//! standalone virtual view (Git history). A plugin implements the methods it
//! really supports; `#[fiex_sdk::filesystem]` records exactly those methods as
//! its capability set and generates the dispatcher.
//!
//! ```ignore
//! use fiex_plugin_sdk as sdk;
//! use sdk::filesystem::{FileSystemProvider, FsEntry, FsListing, FsRead, FsRequest};
//!
//! struct Drive;
//!
//! #[sdk::filesystem]
//! impl FileSystemProvider for Drive {
//!     fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, sdk::RpcError> {
//!         Ok(FsListing::default())
//!     }
//!     fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, sdk::RpcError> {
//!         FsEntry::directory("root", "/").ok_or_else(|| sdk::RpcError::internal("bad id"))
//!     }
//! }
//! ```
//!
//! A provider that only writes `list` and `stat` never claims `read`, `write`,
//! `mkdir`, `rename`, `remove` or `search`. Core hides or rejects those
//! operations from the same declaration; per-node restrictions travel in
//! [`FsEntry::allowed_ops`].
use crate::{RequestContext, RpcError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

/// Maximum bytes a single `fs.readStream` chunk may return.
pub const MAX_CHUNK: usize = 4 * 1024 * 1024;

/// One capability a file-system provider can implement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FsOp {
    List,
    Stat,
    Read,
    Write,
    Mkdir,
    Rename,
    Remove,
    Search,
}

impl FsOp {
    pub const ALL: [FsOp; 8] = [
        FsOp::List,
        FsOp::Stat,
        FsOp::Read,
        FsOp::Write,
        FsOp::Mkdir,
        FsOp::Rename,
        FsOp::Remove,
        FsOp::Search,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FsOp::List => "list",
            FsOp::Stat => "stat",
            FsOp::Read => "read",
            FsOp::Write => "write",
            FsOp::Mkdir => "mkdir",
            FsOp::Rename => "rename",
            FsOp::Remove => "remove",
            FsOp::Search => "search",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        FsOp::ALL.into_iter().find(|op| op.as_str() == value)
    }

    /// Wire methods unlocked by this capability.
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            FsOp::List => &["fs.list"],
            FsOp::Stat => &["fs.stat"],
            FsOp::Read => &["fs.openRead", "fs.readStream", "fs.closeRead"],
            FsOp::Write => &["fs.beginWrite", "fs.writeStream", "fs.commit", "fs.abort"],
            FsOp::Mkdir => &["fs.mkdir"],
            FsOp::Rename => &["fs.rename"],
            FsOp::Remove => &["fs.remove"],
            FsOp::Search => &["fs.search"],
        }
    }
}

impl Serialize for FsOp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FsOp {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        FsOp::parse(&value)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown fs operation: {value}")))
    }
}

/// Kind of a directory entry. Symlinks are reported as their own kind so Core
/// never has to guess whether a target was followed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FsKind {
    #[default]
    File,
    Directory,
    Symlink,
}

/// Metadata for one node. `node_id` is opaque to Core and stable for the
/// lifetime of the mount; it is never a host path.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsEntry {
    pub node_id: String,
    pub name: String,
    #[serde(default)]
    pub kind: FsKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    /// Changes whenever the content changes; required for conditional writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    /// Operations this particular node allows, narrower than the provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_ops: Option<Vec<FsOp>>,
    /// Set by `search` results: location relative to the searched root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
}

impl FsEntry {
    pub fn file(node_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            name: name.into(),
            kind: FsKind::File,
            ..Self::default()
        }
    }

    pub fn directory(node_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            name: name.into(),
            kind: FsKind::Directory,
            ..Self::default()
        }
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    pub fn readonly(mut self) -> Self {
        self.readonly = Some(true);
        self
    }

    pub fn allowing(mut self, ops: impl IntoIterator<Item = FsOp>) -> Self {
        self.allowed_ops = Some(ops.into_iter().collect());
        self
    }

    pub fn is_dir(&self) -> bool {
        self.kind == FsKind::Directory
    }
}

/// One page of a directory listing or a search result.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsListing {
    #[serde(default)]
    pub entries: Vec<FsEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

impl FsListing {
    pub fn new(entries: impl IntoIterator<Item = FsEntry>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
            ..Self::default()
        }
    }
}

/// A parsed `fs.*` request. Providers read only the fields their operation
/// needs; the raw params stay available for extensions.
#[derive(Clone)]
pub struct FsRequest<'a> {
    ctx: &'a RequestContext,
    op: FsOp,
    params: Value,
}

impl<'a> FsRequest<'a> {
    pub fn new(ctx: &'a RequestContext, op: FsOp) -> Self {
        Self {
            ctx,
            op,
            params: ctx.params().clone(),
        }
    }

    pub fn op(&self) -> FsOp {
        self.op
    }

    pub fn ctx(&self) -> &'a RequestContext {
        self.ctx
    }

    pub fn params(&self) -> &Value {
        &self.params
    }

    /// The node the operation targets, when the operation is node-scoped.
    pub fn node_id(&self) -> Option<&str> {
        self.params
            .get("nodeId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    }

    /// Parent directory for `mkdir` and for creating a new file.
    pub fn parent_id(&self) -> Option<&str> {
        self.params
            .get("parentId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    }

    /// New name for `mkdir`/`rename`, or the leaf name of a new file.
    pub fn name(&self) -> Option<&str> {
        self.params.get("name").and_then(Value::as_str)
    }

    /// Optimistic-concurrency token the provider must compare before writing.
    pub fn if_revision(&self) -> Option<&str> {
        self.params.get("ifRevision").and_then(Value::as_str)
    }

    /// Expected content length of a new file, when Core knows it up front.
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

    /// Mount identity, when Core assigned one.
    pub fn mount_id(&self) -> Option<&str> {
        self.params.get("mountId").and_then(Value::as_str)
    }

    /// Connection configuration, for connection-backed mounts.
    pub fn connection(&self) -> Option<&Value> {
        self.params.get("connection")
    }

    /// Contribution identity of a standalone view.
    pub fn contribution_id(&self) -> Option<&str> {
        self.params.get("contributionId").and_then(Value::as_str)
    }

    /// Opaque view context of a standalone view.
    pub fn view_context(&self) -> Option<&Value> {
        self.params.get("viewContext")
    }
}

/// A readable stream returned by [`FileSystemProvider::read`].
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

/// A writable stream returned by [`FileSystemProvider::write`].
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
    /// becomes the `fs.commit` result.
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

fn unsupported(op: FsOp) -> RpcError {
    let mut error = RpcError::method_not_found(op.methods()[0]);
    error.message = format!("Unsupported fs operation: {}", op.as_str());
    error
}

/// The trait a plugin implements for a connection or a standalone view.
///
/// Every method has an `unsupported` default. [`FsDispatch`] is generated by
/// `#[fiex_sdk::filesystem]` from the methods that actually appear in the
/// `impl` block, so defaults never leak into the advertised capability set.
pub trait FileSystemProvider: Send + Sync {
    fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        Err(unsupported(request.op()))
    }

    fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        Err(unsupported(request.op()))
    }

    fn read(&self, request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
        Err(unsupported(request.op()))
    }

    fn write(&self, request: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
        Err(unsupported(request.op()))
    }

    fn mkdir(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        Err(unsupported(request.op()))
    }

    fn rename(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        Err(unsupported(request.op()))
    }

    fn remove(&self, request: &FsRequest<'_>) -> Result<(), RpcError> {
        Err(unsupported(request.op()))
    }

    fn search(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        Err(unsupported(request.op()))
    }
}

/// Generated by `#[fiex_sdk::filesystem]`. It records the implemented methods
/// and routes a parsed request to them.
pub trait FsDispatch: Send + Sync {
    /// Operations the provider really implements.
    fn operations(&self) -> &'static [FsOp];

    /// Run one logical operation. Only operations returned by
    /// [`FsDispatch::operations`] are ever called.
    fn invoke(&self, request: &FsRequest<'_>) -> Result<FsProduct, RpcError>;
}

/// Stream registry and wire adapter shared by every file-system provider.
///
/// The provider owns the `Read`/`Write` implementations; the SDK owns stream
/// ids, chunk offsets, close semantics and commit ordering.
pub struct FsServer {
    next: AtomicU64,
    streams: Mutex<HashMap<String, Stream>>,
}

enum Stream {
    Read {
        reader: Box<dyn Read + Send>,
        offset: u64,
    },
    Write {
        write: FsWrite,
        offset: u64,
    },
}

impl Default for FsServer {
    fn default() -> Self {
        Self {
            next: AtomicU64::new(1),
            streams: Mutex::new(HashMap::new()),
        }
    }
}

impl FsServer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Capability declaration returned from `plugin.initialize`.
    pub fn capability(provider: &dyn FsDispatch) -> Value {
        json!({
            "fs": {
                "operations": provider
                    .operations()
                    .iter()
                    .map(|op| op.as_str())
                    .collect::<Vec<_>>(),
            }
        })
    }

    /// Dispatch one `fs.*` method. Returns `None` for methods outside `fs.*`
    /// so the plugin's own handler can serve them.
    pub fn serve(
        &self,
        provider: &dyn FsDispatch,
        ctx: &RequestContext,
    ) -> Result<Option<Value>, RpcError> {
        let Some(op) = wire_op(ctx.method()) else {
            return Ok(None);
        };
        let value = match ctx.method() {
            "fs.openRead" => {
                let request = FsRequest::new(ctx, op);
                require(provider, op)?;
                let read = match provider.invoke(&request)? {
                    FsProduct::Read(read) => read,
                    _ => return Err(RpcError::internal("provider returned a non-read product")),
                };
                let id = self.mint("read");
                self.lock()?.insert(
                    id.clone(),
                    Stream::Read {
                        reader: read.reader,
                        offset: 0,
                    },
                );
                json!({"streamId": id, "size": read.size})
            }
            "fs.readStream" => {
                let id = param_str(ctx, "streamId")?;
                let offset = ctx.params()["offset"].as_u64().unwrap_or(0);
                let max_bytes = ctx.params()["maxBytes"]
                    .as_u64()
                    .unwrap_or(256 * 1024 + 1)
                    .clamp(1, MAX_CHUNK as u64) as usize;
                let mut streams = self.lock()?;
                let Stream::Read {
                    reader,
                    offset: cursor,
                } = streams
                    .get_mut(id)
                    .ok_or_else(|| RpcError::invalid_params("unknown streamId"))?
                else {
                    return Err(RpcError::invalid_params("stream is not readable"));
                };
                if offset < *cursor {
                    return Err(RpcError::invalid_params("stream cannot seek backwards"));
                }
                skip_to(reader, cursor, offset)?;
                let (bytes, eof) = read_chunk(reader, max_bytes)?;
                *cursor = offset + bytes.len() as u64;
                use base64::{engine::general_purpose::STANDARD, Engine};
                json!({"base64": STANDARD.encode(&bytes), "eof": eof})
            }
            "fs.closeRead" => {
                let id = param_str(ctx, "streamId")?;
                self.lock()?.remove(id);
                Value::Null
            }
            "fs.beginWrite" => {
                let request = FsRequest::new(ctx, op);
                require(provider, op)?;
                let write = match provider.invoke(&request)? {
                    FsProduct::Write(write) => write,
                    _ => return Err(RpcError::internal("provider returned a non-write product")),
                };
                let id = self.mint("write");
                self.lock()?
                    .insert(id.clone(), Stream::Write { write, offset: 0 });
                json!({"streamId": id})
            }
            "fs.writeStream" => {
                let id = param_str(ctx, "streamId")?.to_string();
                let offset = ctx.params()["offset"].as_u64().unwrap_or(0);
                let bytes = decode_chunk(ctx)?;
                let mut guard = self.lock()?;
                let Stream::Write {
                    write,
                    offset: written,
                } = guard
                    .get_mut(&id)
                    .ok_or_else(|| RpcError::invalid_params("unknown streamId"))?
                else {
                    return Err(RpcError::invalid_params("stream is not writable"));
                };
                if offset != *written {
                    return Err(RpcError::invalid_params(
                        "write stream must advance sequentially",
                    ));
                }
                write
                    .writer
                    .write_all(&bytes)
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                *written += bytes.len() as u64;
                json!({"written": bytes.len()})
            }
            "fs.commit" => {
                let id = param_str(ctx, "streamId")?.to_string();
                let stream = self
                    .lock()?
                    .remove(&id)
                    .ok_or_else(|| RpcError::invalid_params("unknown streamId"))?;
                let Stream::Write { write, .. } = stream else {
                    return Err(RpcError::invalid_params("stream is not writable"));
                };
                match write.commit()? {
                    Some(entry) => serde_json::to_value(entry)
                        .map_err(|error| RpcError::internal(error.to_string()))?,
                    None => json!({}),
                }
            }
            "fs.abort" => {
                let id = param_str(ctx, "streamId")?;
                if let Some(Stream::Write { write, .. }) = self.lock()?.remove(id) {
                    write.abort();
                }
                Value::Null
            }
            _ => {
                let request = FsRequest::new(ctx, op);
                require(provider, op)?;
                match provider.invoke(&request)? {
                    FsProduct::Entry(entry) => serde_json::to_value(entry)
                        .map_err(|error| RpcError::internal(error.to_string()))?,
                    FsProduct::Listing(listing) => serde_json::to_value(listing)
                        .map_err(|error| RpcError::internal(error.to_string()))?,
                    FsProduct::Done | FsProduct::Read(_) | FsProduct::Write(_) => json!({}),
                }
            }
        };
        Ok(Some(value))
    }

    fn mint(&self, kind: &str) -> String {
        format!("{kind}-{}", self.next.fetch_add(1, Ordering::Relaxed))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, Stream>>, RpcError> {
        self.streams
            .lock()
            .map_err(|_| RpcError::internal("stream registry unavailable"))
    }
}

fn wire_op(method: &str) -> Option<FsOp> {
    FsOp::ALL
        .into_iter()
        .find(|op| op.methods().contains(&method))
}

fn require(provider: &dyn FsDispatch, op: FsOp) -> Result<(), RpcError> {
    if provider.operations().contains(&op) {
        Ok(())
    } else {
        let mut error = unsupported(op);
        error.data = Some(json!({"operation": op.as_str()}));
        Err(error)
    }
}

fn param_str<'a>(ctx: &'a RequestContext, key: &str) -> Result<&'a str, RpcError> {
    ctx.params()
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(format!("missing {key}")))
}

fn decode_chunk(ctx: &RequestContext) -> Result<Vec<u8>, RpcError> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let encoded = param_str(ctx, "base64")?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| RpcError::invalid_params("invalid base64 chunk"))?;
    if bytes.len() > MAX_CHUNK {
        return Err(RpcError::invalid_params("chunk exceeds limit"));
    }
    Ok(bytes)
}

fn skip_to(
    reader: &mut Box<dyn Read + Send>,
    cursor: &mut u64,
    offset: u64,
) -> Result<(), RpcError> {
    let mut skip = offset - *cursor;
    let mut scratch = [0u8; 8192];
    while skip > 0 {
        let want = skip.min(scratch.len() as u64) as usize;
        let read = reader
            .read(&mut scratch[..want])
            .map_err(|error| RpcError::internal(error.to_string()))?;
        if read == 0 {
            return Err(RpcError::internal("stream ended before requested offset"));
        }
        skip -= read as u64;
        *cursor += read as u64;
    }
    Ok(())
}

fn read_chunk(
    reader: &mut Box<dyn Read + Send>,
    max_bytes: usize,
) -> Result<(Vec<u8>, bool), RpcError> {
    let mut bytes = vec![0u8; max_bytes];
    let mut read = 0;
    while read < max_bytes {
        match reader.read(&mut bytes[read..]) {
            Ok(0) => break,
            Ok(count) => read += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(RpcError::internal(error.to_string())),
        }
    }
    bytes.truncate(read);
    Ok((bytes, read == 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        context::ContextInner,
        runtime::{session::Session, HostLink},
        tasks::TaskManager,
    };
    use std::{
        io::Cursor,
        sync::{atomic::AtomicBool, Arc},
    };

    struct NoopLink;
    impl HostLink for NoopLink {
        fn call(&self, method: &str, _params: Value) -> Result<Value, RpcError> {
            Err(RpcError::method_not_found(method))
        }
        fn closed(&self) -> bool {
            false
        }
    }

    fn request(method: &str, params: Value) -> RequestContext {
        RequestContext::new(
            ContextInner::new(
                Arc::new(NoopLink),
                Arc::new(TaskManager::new(2)),
                Session::Operation(1),
                Arc::new(AtomicBool::new(false)),
            ),
            method.into(),
            params,
        )
    }

    struct Memory {
        writable: bool,
    }

    #[crate::filesystem]
    impl FileSystemProvider for Memory {
        fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
            Ok(FsListing::new([
                FsEntry::directory("root", "/"),
                FsEntry::file("a", format!("a-{}", request.node_id().unwrap_or("-")))
                    .with_size(4)
                    .with_revision("r1"),
            ]))
        }

        fn stat(&self, _request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
            Ok(FsEntry::file("a", "a.txt").with_size(4).with_revision("r1"))
        }

        fn read(&self, _request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
            Ok(FsRead::from_bytes(b"abcd".to_vec()))
        }

        fn write(&self, request: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
            if !self.writable {
                return Err(RpcError::method_not_found("fs.beginWrite"));
            }
            let revision = format!("r-{}", request.if_revision().unwrap_or("initial"));
            Ok(FsWrite::with_commit(Cursor::new(Vec::new()), move || {
                Ok(FsEntry::file("a", "a.txt").with_revision(revision))
            }))
        }
    }

    struct ReadOnly;

    #[crate::filesystem]
    impl FileSystemProvider for ReadOnly {
        fn list(&self, _request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
            Ok(FsListing::default())
        }
    }

    #[test]
    fn capabilities_follow_the_impl_block() {
        let memory = Memory { writable: true };
        assert_eq!(
            memory.operations(),
            &[FsOp::List, FsOp::Stat, FsOp::Read, FsOp::Write]
        );
        let readonly = ReadOnly;
        assert_eq!(readonly.operations(), &[FsOp::List]);
        assert_eq!(
            FsServer::capability(&readonly),
            json!({"fs": {"operations": ["list"]}})
        );
    }

    #[test]
    fn unimplemented_operations_are_refused_before_the_provider() {
        let server = FsServer::new();
        let provider = ReadOnly;
        let error = server
            .serve(&provider, &request("fs.remove", json!({"nodeId": "x"})))
            .unwrap_err();
        assert_eq!(error.code, -32601);
        assert_eq!(error.data.unwrap()["operation"], "remove");
    }

    #[test]
    fn non_fs_methods_fall_through() {
        let server = FsServer::new();
        assert!(server
            .serve(&ReadOnly, &request("custom.thing", json!({})))
            .unwrap()
            .is_none());
    }

    #[test]
    fn read_stream_honours_offsets_and_rejects_backward_seeks() {
        let server = FsServer::new();
        let opened = server
            .serve(
                &Memory { writable: true },
                &request("fs.openRead", json!({"nodeId": "a"})),
            )
            .unwrap()
            .unwrap();
        assert_eq!(opened["size"], 4);
        let id = opened["streamId"].as_str().unwrap();
        let first = server
            .serve(
                &Memory { writable: true },
                &request(
                    "fs.readStream",
                    json!({"streamId": id, "offset": 0, "maxBytes": 2}),
                ),
            )
            .unwrap()
            .unwrap();
        assert_eq!(first["base64"], "YWI=");
        let second = server
            .serve(
                &Memory { writable: true },
                &request(
                    "fs.readStream",
                    json!({"streamId": id, "offset": 2, "maxBytes": 2}),
                ),
            )
            .unwrap()
            .unwrap();
        assert_eq!(second["base64"], "Y2Q=");
        assert!(server
            .serve(
                &Memory { writable: true },
                &request(
                    "fs.readStream",
                    json!({"streamId": id, "offset": 1, "maxBytes": 2}),
                ),
            )
            .is_err());
        server
            .serve(
                &Memory { writable: true },
                &request("fs.closeRead", json!({"streamId": id})),
            )
            .unwrap()
            .unwrap();
    }

    #[test]
    fn writes_commit_through_the_provider_callback() {
        let server = FsServer::new();
        let provider = Memory { writable: true };
        let opened = server
            .serve(
                &provider,
                &request("fs.beginWrite", json!({"nodeId": "a", "ifRevision": "r1"})),
            )
            .unwrap()
            .unwrap();
        let id = opened["streamId"].as_str().unwrap().to_string();
        server
            .serve(
                &provider,
                &request(
                    "fs.writeStream",
                    json!({"streamId": id, "offset": 0, "base64": "YWJjZA=="}),
                ),
            )
            .unwrap();
        let committed = server
            .serve(&provider, &request("fs.commit", json!({"streamId": id})))
            .unwrap()
            .unwrap();
        assert_eq!(committed["revision"], "r-r1");

        let reopened = server
            .serve(&provider, &request("fs.beginWrite", json!({"nodeId": "a"})))
            .unwrap()
            .unwrap();
        let id = reopened["streamId"].as_str().unwrap().to_string();
        server
            .serve(&provider, &request("fs.abort", json!({"streamId": id})))
            .unwrap();
        assert!(server
            .serve(&provider, &request("fs.commit", json!({"streamId": id})))
            .is_err());
    }

    #[test]
    fn provider_without_write_capability_cannot_begin_a_write() {
        let server = FsServer::new();
        let error = server
            .serve(&ReadOnly, &request("fs.beginWrite", json!({"nodeId": "a"})))
            .unwrap_err();
        assert_eq!(error.data.unwrap()["operation"], "write");
    }
}
