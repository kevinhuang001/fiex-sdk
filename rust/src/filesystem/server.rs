use super::*;
use crate::{RequestContext, RpcError, context::StreamSession};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
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
        mount_id: Option<String>,
        reader: Box<dyn Read + Send>,
        offset: u64,
        /// Keeps host handles opened while creating the stream authorized until
        /// `vfs.closeRead`, not just for the `vfs.openRead` request. Only its
        /// `Drop` matters here.
        #[allow(dead_code)]
        session: Option<StreamSession>,
    },
    Write {
        mount_id: Option<String>,
        write: FsWrite,
        offset: u64,
        /// Keeps the upload's host handles authorized until `vfs.commit`/`abort`.
        session: Option<StreamSession>,
    },
}

fn stream_mount_id(ctx: &RequestContext) -> Option<String> {
    ctx.params()
        .get("mountId")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

impl Stream {
    /// Stream IDs are scoped to their Core-verified mount even when a single
    /// SDK provider instance serves multiple accounts.
    fn belongs_to(&self, ctx: &RequestContext) -> bool {
        let expected = match self {
            Self::Read { mount_id, .. } | Self::Write { mount_id, .. } => mount_id.as_deref(),
        };
        expected == ctx.params().get("mountId").and_then(Value::as_str)
    }

    fn read_owner(&self, ctx: &RequestContext) -> bool {
        matches!(self, Self::Read { .. }) && self.belongs_to(ctx)
    }

    fn write_owner(&self, ctx: &RequestContext) -> bool {
        matches!(self, Self::Write { .. }) && self.belongs_to(ctx)
    }
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
    pub fn capability(provider: &dyn VfsProvider) -> Result<Value, RpcError> {
        let handlers = provider
            .preflight_handlers()
            .map_err(|error| RpcError::internal(error.to_string()))?;
        Ok(json!({
            "vfs": {
                "discovery": true,
                "preflightOps": handlers
                    .operations()
                    .into_iter()
                    .map(|operation| operation.as_str())
                    .collect::<Vec<_>>(),
                "operations": provider
                    .operations()
                    .iter()
                    .map(|op| op.as_str())
                    .collect::<Vec<_>>(),
            }
        }))
    }

    /// Dispatch one `vfs.*` method. Returns `None` for methods outside `vfs.*`
    /// so the plugin's own handler can serve them.
    pub fn serve(
        &self,
        provider: &dyn VfsProvider,
        ctx: &RequestContext,
    ) -> Result<Option<Value>, RpcError> {
        let handlers = provider
            .preflight_handlers()
            .map_err(|error| RpcError::internal(error.to_string()))?;
        self.serve_with_preflight(provider, &handlers, ctx)
    }
}

mod dispatch;
