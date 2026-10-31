use super::*;
use crate::{context::StreamSession, RequestContext, RpcError};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
mod helpers;
use helpers::*;

/// Stream registry and wire adapter shared by every file-system provider.
///
/// The provider owns the `Read`/`Write` implementations; the SDK owns stream
/// ids, chunk offsets, close semantics and commit ordering.
static NEXT_FS_SERVER_ID: AtomicU64 = AtomicU64::new(1);

pub struct FsServer {
    namespace: u64,
    next: AtomicU64,
    streams: Mutex<HashMap<String, Stream>>,
}

enum Stream {
    Read {
        reader: Box<dyn Read + Send>,
        offset: u64,
        /// Keeps host handles opened while creating the stream authorized until
        /// `vfs.closeRead`, not just for the `vfs.openRead` request. Only its
        /// `Drop` matters here.
        #[allow(dead_code)]
        session: Option<StreamSession>,
    },
    Write {
        write: FsWrite,
        offset: u64,
        /// Keeps the upload's host handles authorized until `vfs.commit`/`abort`.
        session: Option<StreamSession>,
    },
}

impl Default for FsServer {
    fn default() -> Self {
        Self {
            namespace: NEXT_FS_SERVER_ID.fetch_add(1, Ordering::Relaxed),
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
    pub fn capability(provider: &dyn VfsProvider) -> Value {
        json!({
            "vfs": {
                "discovery": true,
                "preflight": provider.supports_preflight(),
                "operations": provider
                    .operations()
                    .iter()
                    .map(|op| op.as_str())
                    .collect::<Vec<_>>(),
            }
        })
    }

    /// Dispatch one `vfs.*` method. Returns `None` for methods outside `vfs.*`
    /// so the plugin's own handler can serve them.
    pub fn serve(
        &self,
        provider: &dyn VfsProvider,
        ctx: &RequestContext,
    ) -> Result<Option<Value>, RpcError> {
        if ctx.method() == "vfs.preflight" {
            if !provider.supports_preflight() {
                return Err(RpcError::method_not_found("vfs.preflight"));
            }
            let request: TransferPreflight = serde_json::from_value(ctx.params().clone())
                .map_err(|error| RpcError::invalid_params(error.to_string()))?;
            return serde_json::to_value(provider.preflight(ctx, &request)?)
                .map(Some)
                .map_err(|error| RpcError::internal(error.to_string()));
        }
        if ctx.method() == "vfs.discover" {
            let operations = discover(provider, ctx)?;
            let search = if operations.contains(&FsOperation::Search) {
                provider.search_capabilities(ctx)
            } else {
                SearchCapabilities::default()
            };
            return Ok(Some(json!({"operations": operations, "search": search})));
        }
        let Some(op) = wire_op(ctx.method()) else {
            return Ok(None);
        };
        let value = match ctx.method() {
            "vfs.openRead" => {
                let (product, session) = invoke_stream(provider, ctx, op, "读取文件")?;
                let FsProduct::Read(read) = product else {
                    return Err(RpcError::internal("provider returned a non-read product"));
                };
                let id = self.mint("read");
                self.lock()?.insert(
                    id.clone(),
                    Stream::Read {
                        reader: read.reader,
                        offset: 0,
                        session,
                    },
                );
                json!({"streamId": id, "size": read.size})
            }
            "vfs.readStream" => {
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
                    ..
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
            "vfs.closeRead" => {
                let id = param_str(ctx, "streamId")?;
                // Drop the stream outside the registry lock: releasing the
                // session is a host call.
                let removed = self.lock()?.remove(id);
                drop(removed);
                Value::Null
            }
            "vfs.beginWrite" => {
                let (product, session) = invoke_stream(provider, ctx, op, "写入文件")?;
                let FsProduct::Write(write) = product else {
                    return Err(RpcError::internal("provider returned a non-write product"));
                };
                let id = self.mint("write");
                self.lock()?.insert(
                    id.clone(),
                    Stream::Write {
                        write,
                        offset: 0,
                        session,
                    },
                );
                json!({"streamId": id})
            }
            "vfs.writeStream" => {
                let id = param_str(ctx, "streamId")?.to_string();
                let offset = ctx.params()["offset"].as_u64().unwrap_or(0);
                let bytes = decode_chunk(ctx)?;
                let mut guard = self.lock()?;
                let Stream::Write {
                    write,
                    offset: written,
                    ..
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
            "vfs.commit" => {
                let id = param_str(ctx, "streamId")?.to_string();
                let stream = self
                    .lock()?
                    .remove(&id)
                    .ok_or_else(|| RpcError::invalid_params("unknown streamId"))?;
                let Stream::Write { write, session, .. } = stream else {
                    return Err(RpcError::invalid_params("stream is not writable"));
                };
                let committed = write.commit();
                drop(session);
                match committed? {
                    Some(entry) => serde_json::to_value(entry)
                        .map_err(|error| RpcError::internal(error.to_string()))?,
                    None => json!({}),
                }
            }
            "vfs.abort" => {
                let id = param_str(ctx, "streamId")?;
                let removed = self.lock()?.remove(id);
                if let Some(Stream::Write { write, session, .. }) = removed {
                    write.abort();
                    drop(session);
                }
                Value::Null
            }
            _ => {
                let request = FsRequest::new(ctx, op);
                let available = require(provider, ctx, op)?;
                if op == FsOperation::Search
                    && !provider
                        .search_capabilities(ctx)
                        .supports(request.search_mode()?)
                {
                    return Err(unsupported(FsOperation::Search));
                }
                match invoke(provider, &request)? {
                    FsProduct::Entry(mut entry) => {
                        narrow_entry(&available, &mut entry);
                        serde_json::to_value(entry)
                            .map_err(|error| RpcError::internal(error.to_string()))?
                    }
                    FsProduct::Listing(listing) => serde_json::to_value(listing)
                        .map_err(|error| RpcError::internal(error.to_string()))?,
                    FsProduct::Done | FsProduct::Read(_) | FsProduct::Write(_) => json!({}),
                }
            }
        };
        Ok(Some(value))
    }

    fn mint(&self, kind: &str) -> String {
        format!(
            "{}-{kind}-{}",
            self.namespace,
            self.next.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, Stream>>, RpcError> {
        self.streams
            .lock()
            .map_err(|_| RpcError::internal("stream registry unavailable"))
    }
}
