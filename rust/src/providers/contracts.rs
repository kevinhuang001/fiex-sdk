//! Public provider-specific traits implemented by resident plugin processes.
use crate::{InfoSection, PreviewView, RequestContext, RpcError};
use serde_json::{json, Value};

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

/// `connectionContextMenu.resolve` / `.execute`: flat actions for one saved
/// connection. The request carries a credential-free connection summary.
pub trait ConnectionContextMenuProvider: Send + Sync {
    fn resolve(&self, ctx: &RequestContext) -> Result<Vec<Value>, RpcError>;
    fn execute(&self, ctx: &RequestContext) -> Result<Value, RpcError>;
}

/// `search.query`: local entries are paths relative to the authorized root;
/// standalone VFS providers return the same entry shape as `vfs.search`.
pub use fiex_provider_contracts::SearchResults;

pub trait SearchProvider: Send + Sync {
    /// A local search provider can advertise one or both modes. Core filters
    /// candidates before calling; this check also protects direct RPC calls.
    fn supports(&self, mode: crate::filesystem::SearchMode) -> bool {
        mode == crate::filesystem::SearchMode::Name
    }
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

/// Credentials returned to Core during validation are persisted in the selected
/// credential backend and excluded from connection config responses to the UI.
#[derive(serde::Serialize)]
pub struct ConnectionValidation {
    pub valid: bool,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub credentials: std::collections::BTreeMap<String, String>,
}
impl ConnectionValidation {
    pub fn valid() -> Self {
        Self {
            valid: true,
            credentials: Default::default(),
        }
    }
    pub fn credential(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.credentials.insert(name.into(), value.into());
        self
    }
    pub fn into_value(self) -> Value {
        json!(self)
    }
}

/// Connection lifecycle: validation plus the navigation target a connection
/// opens. File-system work lives in [`crate::filesystem::VfsProvider`],
/// which connections and standalone views share.
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
}

pub(super) fn unsupported(ctx: &RequestContext) -> RpcError {
    RpcError::method_not_found(ctx.method())
}
