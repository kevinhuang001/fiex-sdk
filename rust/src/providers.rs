//! Optional typed event providers.
//!
//! A plugin can keep its existing `sdk::serve` dispatcher, or register only the
//! system events it implements here. Custom RPC methods still go through the
//! fallback handler. Provider implementations stay optional and composable;
//! nothing forces a plugin to implement events it does not own.
use crate::{InfoSection, PreviewView, RequestContext, RpcError};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::Read,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

/// `info.resolve` rows, grouped into sections by Core.
pub trait InfoProvider: Send + Sync {
    fn resolve(&self, ctx: &RequestContext) -> Result<Vec<InfoSection>, RpcError>;
}

/// `preview.open`: return a package view, text, or an explicit unsupported
/// result. Core reads the package entry and creates the preview session.
pub trait PreviewProvider: Send + Sync {
    fn open(&self, ctx: &RequestContext) -> Result<PreviewView, RpcError>;
}

/// `decorations.resolve`: return validated decoration values.
pub trait DecorationProvider: Send + Sync {
    fn resolve(&self, ctx: &RequestContext) -> Result<Vec<Value>, RpcError>;
}

/// `contextMenu.resolve` and `contextMenu.execute`. Both share one registration
/// because Core only executes items that the same provider resolved.
pub trait ContextMenuProvider: Send + Sync {
    fn resolve(&self, ctx: &RequestContext) -> Result<Vec<Value>, RpcError>;
    fn execute(&self, ctx: &RequestContext) -> Result<Value, RpcError>;
}

/// `search.query`: local entries are paths relative to the authorized root;
/// standalone VFS providers return the same entry shape as `vfs.search`.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SearchResults {
    pub entries: Vec<Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

pub trait SearchProvider: Send + Sync {
    fn query(&self, ctx: &RequestContext) -> Result<SearchResults, RpcError>;
}

/// `hooks.invoke`: return `{handled: bool, preview?: bool}`.
pub trait HookProvider: Send + Sync {
    fn invoke(&self, ctx: &RequestContext) -> Result<Value, RpcError>;
}

/// `toolbar.invoke`: a toolbar click's result.
pub trait ToolbarProvider: Send + Sync {
    fn invoke(&self, ctx: &RequestContext) -> Result<Value, RpcError>;
}

/// A reader returned by [`ConnectionProvider::open_read`]. Core drives the
/// `fs.readStream` offset/chunk lifecycle; dropping the stream closes it.
pub struct ConnectionReader {
    pub reader: Box<dyn Read + Send>,
    pub size: Option<u64>,
}

impl ConnectionReader {
    pub fn new(reader: impl Read + Send + 'static, size: Option<u64>) -> Self {
        Self {
            reader: Box::new(reader),
            size,
        }
    }
}

/// Connection lifecycle and filesystem events. The default reader serves
/// `fs.openRead`/`fs.readStream`/`fs.closeRead` from a standard `Read`; a
/// provider that needs its own framing can still dispatch those methods itself.
pub trait ConnectionProvider: Send + Sync {
    fn validate(&self, ctx: &RequestContext) -> Result<Value, RpcError> {
        Err(unsupported(ctx))
    }
    fn open(&self, ctx: &RequestContext) -> Result<Value, RpcError> {
        Err(unsupported(ctx))
    }
    fn create(&self, ctx: &RequestContext) -> Result<Value, RpcError> {
        Err(unsupported(ctx))
    }
    fn list(&self, ctx: &RequestContext) -> Result<Value, RpcError> {
        Err(unsupported(ctx))
    }
    fn stat(&self, ctx: &RequestContext) -> Result<Value, RpcError> {
        Err(unsupported(ctx))
    }
    fn write(&self, ctx: &RequestContext) -> Result<Value, RpcError> {
        Err(unsupported(ctx))
    }
    fn open_read(&self, ctx: &RequestContext) -> Result<ConnectionReader, RpcError> {
        Err(unsupported(ctx))
    }
}

/// Standalone `extensions.vfs` provider events. Entry DTOs stay JSON because
/// Core binds every returned `nodeId` to its own mount identity.
pub trait VfsProvider: Send + Sync {
    fn list(&self, ctx: &RequestContext) -> Result<Vec<Value>, RpcError> {
        Err(unsupported(ctx))
    }
    fn stat(&self, ctx: &RequestContext) -> Result<Value, RpcError> {
        Err(unsupported(ctx))
    }
    fn read_small(&self, ctx: &RequestContext) -> Result<Vec<u8>, RpcError> {
        Err(unsupported(ctx))
    }
    fn search(&self, ctx: &RequestContext) -> Result<Value, RpcError> {
        Err(unsupported(ctx))
    }
}

fn unsupported(ctx: &RequestContext) -> RpcError {
    RpcError::method_not_found(ctx.method())
}

/// Collects optional providers and dispatches their fixed Core event methods.
/// Methods without a registered provider fall through to the plugin's own
/// handler, so custom RPC keeps working unchanged.
struct StreamReader {
    reader: Box<dyn Read + Send>,
    offset: u64,
}

#[derive(Default)]
pub struct Providers {
    readers: Arc<Mutex<HashMap<String, StreamReader>>>,
    info: Option<Box<dyn InfoProvider>>,
    preview: Option<Box<dyn PreviewProvider>>,
    decorations: Option<Box<dyn DecorationProvider>>,
    context_menu: Option<Box<dyn ContextMenuProvider>>,
    search: Option<Box<dyn SearchProvider>>,
    hooks: Option<Box<dyn HookProvider>>,
    toolbar: Option<Box<dyn ToolbarProvider>>,
    connection: Option<Box<dyn ConnectionProvider>>,
    vfs: Option<Box<dyn VfsProvider>>,
}

impl Providers {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn info(mut self, provider: impl InfoProvider + 'static) -> Self {
        self.info = Some(Box::new(provider));
        self
    }
    pub fn preview(mut self, provider: impl PreviewProvider + 'static) -> Self {
        self.preview = Some(Box::new(provider));
        self
    }
    pub fn decorations(mut self, provider: impl DecorationProvider + 'static) -> Self {
        self.decorations = Some(Box::new(provider));
        self
    }
    pub fn context_menu(mut self, provider: impl ContextMenuProvider + 'static) -> Self {
        self.context_menu = Some(Box::new(provider));
        self
    }
    pub fn search(mut self, provider: impl SearchProvider + 'static) -> Self {
        self.search = Some(Box::new(provider));
        self
    }
    pub fn hooks(mut self, provider: impl HookProvider + 'static) -> Self {
        self.hooks = Some(Box::new(provider));
        self
    }
    pub fn toolbar(mut self, provider: impl ToolbarProvider + 'static) -> Self {
        self.toolbar = Some(Box::new(provider));
        self
    }
    pub fn connection(mut self, provider: impl ConnectionProvider + 'static) -> Self {
        self.connection = Some(Box::new(provider));
        self
    }
    pub fn vfs(mut self, provider: impl VfsProvider + 'static) -> Self {
        self.vfs = Some(Box::new(provider));
        self
    }

    /// Run a provider server. Unregistered methods are delegated to `fallback`.
    pub fn serve_with<F>(self, plugin_id: &str, fallback: F) -> std::io::Result<()>
    where
        F: Fn(RequestContext) -> Result<Value, RpcError> + Send + Sync + 'static,
    {
        crate::serve(plugin_id, move |ctx| match self.dispatch(&ctx)? {
            Some(value) => Ok(value),
            None => fallback(ctx),
        })
    }

    /// Run a provider server with no custom RPC methods.
    pub fn serve(self, plugin_id: &str) -> std::io::Result<()> {
        self.serve_with(plugin_id, |ctx| {
            Err(RpcError::method_not_found(ctx.method()))
        })
    }

    fn dispatch(&self, ctx: &RequestContext) -> Result<Option<Value>, RpcError> {
        let value = match ctx.method() {
            "info.resolve" => {
                let sections = self
                    .info
                    .as_ref()
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"sections":sections})
            }
            "preview.open" => {
                let view = self
                    .preview
                    .as_ref()
                    .ok_or_else(|| unsupported(ctx))?
                    .open(ctx)?;
                serde_json::to_value(view).map_err(|error| RpcError::internal(error.to_string()))?
            }
            "decorations.resolve" => {
                let decorations = self
                    .decorations
                    .as_ref()
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"decorations":decorations})
            }
            "contextMenu.resolve" => {
                let items = self
                    .context_menu
                    .as_ref()
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"items":items})
            }
            "contextMenu.execute" => self
                .context_menu
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .execute(ctx)?,
            "search.query" => serde_json::to_value(
                self.search
                    .as_ref()
                    .ok_or_else(|| unsupported(ctx))?
                    .query(ctx)?,
            )
            .map_err(|error| RpcError::internal(error.to_string()))?,
            "hooks.invoke" => self
                .hooks
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .invoke(ctx)?,
            "toolbar.invoke" => self
                .toolbar
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .invoke(ctx)?,
            "connection.validate" => self
                .connection
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .validate(ctx)?,
            "connection.open" => self
                .connection
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .open(ctx)?,
            "connection.create" => self
                .connection
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .create(ctx)?,
            "fs.list" => self
                .connection
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .list(ctx)?,
            "fs.stat" => self
                .connection
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .stat(ctx)?,
            "fs.writeFile" => self
                .connection
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .write(ctx)?,
            "fs.openRead" => {
                let opened = self
                    .connection
                    .as_ref()
                    .ok_or_else(|| unsupported(ctx))?
                    .open_read(ctx)?;
                static NEXT: AtomicU64 = AtomicU64::new(1);
                let id = format!("provider-read-{}", NEXT.fetch_add(1, Ordering::Relaxed));
                self.readers
                    .lock()
                    .map_err(|_| RpcError::internal("reader registry unavailable"))?
                    .insert(
                        id.clone(),
                        StreamReader {
                            reader: opened.reader,
                            offset: 0,
                        },
                    );
                json!({"streamId":id,"size":opened.size})
            }
            "fs.readStream" => {
                let id = ctx.params()["streamId"]
                    .as_str()
                    .ok_or_else(|| RpcError::invalid_params("missing streamId"))?;
                let offset = ctx.params()["offset"].as_u64().unwrap_or(0);
                let max_bytes = ctx.params()["maxBytes"]
                    .as_u64()
                    .unwrap_or(256 * 1024)
                    .clamp(1, 8 * 1024 * 1024) as usize;
                let mut readers = self
                    .readers
                    .lock()
                    .map_err(|_| RpcError::internal("reader registry unavailable"))?;
                let stream = readers
                    .get_mut(id)
                    .ok_or_else(|| RpcError::invalid_params("unknown streamId"))?;
                if offset < stream.offset {
                    return Err(RpcError::invalid_params("stream cannot seek backwards"));
                }
                let mut skip = offset - stream.offset;
                let mut scratch = [0u8; 8192];
                while skip > 0 {
                    let want = skip.min(scratch.len() as u64) as usize;
                    let read = stream
                        .reader
                        .read(&mut scratch[..want])
                        .map_err(|error| RpcError::internal(error.to_string()))?;
                    if read == 0 {
                        return Err(RpcError::internal("stream ended before requested offset"));
                    }
                    skip -= read as u64;
                    stream.offset += read as u64;
                }
                let mut bytes = vec![0u8; max_bytes];
                let mut read = 0;
                while read < max_bytes {
                    match stream.reader.read(&mut bytes[read..]) {
                        Ok(0) => break,
                        Ok(count) => read += count,
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(RpcError::internal(error.to_string())),
                    }
                }
                bytes.truncate(read);
                stream.offset += read as u64;
                use base64::{engine::general_purpose::STANDARD, Engine};
                json!({"base64":STANDARD.encode(&bytes),"eof":read == 0})
            }
            "fs.closeRead" => {
                let id = ctx.params()["streamId"]
                    .as_str()
                    .ok_or_else(|| RpcError::invalid_params("missing streamId"))?;
                self.readers
                    .lock()
                    .map_err(|_| RpcError::internal("reader registry unavailable"))?
                    .remove(id);
                Value::Null
            }
            "vfs.list" => {
                let entries = self
                    .vfs
                    .as_ref()
                    .ok_or_else(|| unsupported(ctx))?
                    .list(ctx)?;
                json!({"entries":entries})
            }
            "vfs.stat" => self
                .vfs
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .stat(ctx)?,
            "vfs.readSmall" => {
                use base64::{engine::general_purpose::STANDARD, Engine};
                let bytes = self
                    .vfs
                    .as_ref()
                    .ok_or_else(|| unsupported(ctx))?
                    .read_small(ctx)?;
                json!({"base64":STANDARD.encode(bytes)})
            }
            "vfs.search" => self
                .vfs
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .search(ctx)?,
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        context::ContextInner,
        runtime::{session::Session, HostLink},
        tasks::TaskManager,
        InfoRow, InfoSection,
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

    struct MemoryConnection;
    impl ConnectionProvider for MemoryConnection {
        fn open_read(&self, _ctx: &RequestContext) -> Result<ConnectionReader, RpcError> {
            Ok(ConnectionReader::new(
                Cursor::new(b"abcd".to_vec()),
                Some(4),
            ))
        }
    }

    struct ExampleInfo;
    impl InfoProvider for ExampleInfo {
        fn resolve(&self, _ctx: &RequestContext) -> Result<Vec<InfoSection>, RpcError> {
            Ok(vec![InfoSection {
                source: String::new(),
                provider: String::new(),
                plugin_id: None,
                id: "example".into(),
                title: "Example".into(),
                rows: vec![InfoRow {
                    label: "A".into(),
                    value: "B".into(),
                    copyable: false,
                }],
            }])
        }
    }

    #[test]
    fn connection_reader_serves_offsets_and_closes() {
        let providers = Providers::new().connection(MemoryConnection);
        let opened = providers
            .dispatch(&request("fs.openRead", json!({})))
            .unwrap()
            .unwrap();
        assert_eq!(opened["size"], 4);
        let stream = opened["streamId"].as_str().unwrap();
        let first = providers
            .dispatch(&request(
                "fs.readStream",
                json!({"streamId":stream,"offset":0,"maxBytes":2}),
            ))
            .unwrap()
            .unwrap();
        assert_eq!(first["base64"], "YWI=");
        assert_eq!(first["eof"], false);
        let second = providers
            .dispatch(&request(
                "fs.readStream",
                json!({"streamId":stream,"offset":2,"maxBytes":8}),
            ))
            .unwrap()
            .unwrap();
        assert_eq!(second["base64"], "Y2Q=");
        assert!(
            providers
                .dispatch(&request(
                    "fs.readStream",
                    json!({"streamId":stream,"offset":1,"maxBytes":2}),
                ))
                .is_err(),
            "backwards seek must be rejected"
        );
        providers
            .dispatch(&request("fs.closeRead", json!({"streamId":stream})))
            .unwrap();
        assert!(providers
            .dispatch(&request(
                "fs.readStream",
                json!({"streamId":stream,"offset":4,"maxBytes":2}),
            ))
            .is_err());
    }

    #[test]
    fn search_results_omit_false_truncation() {
        let value = serde_json::to_value(SearchResults::default()).unwrap();
        assert_eq!(value, json!({"entries":[]}));
    }

    #[test]
    fn providers_are_composable() {
        let providers = Providers::new().info(ExampleInfo).search(ExampleInfoSearch);
        assert!(providers.info.is_some());
        assert!(providers.search.is_some());
        assert!(providers.preview.is_none());
    }

    struct ExampleInfoSearch;
    impl SearchProvider for ExampleInfoSearch {
        fn query(&self, _ctx: &RequestContext) -> Result<SearchResults, RpcError> {
            Ok(SearchResults::default())
        }
    }
}
