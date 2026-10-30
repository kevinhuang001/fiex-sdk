//! Optional typed event providers.
//!
//! A plugin can keep its existing `sdk::serve` dispatcher, or register only the
//! system events it implements here. Custom RPC methods still go through the
//! fallback handler. Provider implementations stay optional and composable;
//! nothing forces a plugin to implement events it does not own.
use crate::{
    filesystem::{FsServer, VfsProvider},
    InfoSection, PreviewView, RequestContext, RpcError,
};
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
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SearchResults {
    pub entries: Vec<Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    #[serde(
        default,
        rename = "nextCursor",
        skip_serializing_if = "Option::is_none"
    )]
    pub next_cursor: Option<String>,
}

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

fn unsupported(ctx: &RequestContext) -> RpcError {
    RpcError::method_not_found(ctx.method())
}

/// Collects optional providers and dispatches their fixed Core event methods.
/// Methods without a registered provider fall through to the plugin's own
/// handler, so custom RPC keeps working unchanged.
/// One registered file-system provider plus its stream registry.
struct FsProvider {
    dispatch: Box<dyn VfsProvider>,
    server: FsServer,
}

#[derive(Default)]
pub struct Providers {
    diagnostics: bool,
    filesystem: Option<FsProvider>,
    info: Option<Box<dyn InfoProvider>>,
    preview: Option<Box<dyn PreviewProvider>>,
    decorations: Option<Box<dyn DecorationProvider>>,
    context_menu: Option<Box<dyn ContextMenuProvider>>,
    connection_context_menu: Option<Box<dyn ConnectionContextMenuProvider>>,
    search: Option<Box<dyn SearchProvider>>,
    hooks: Option<Box<dyn HookProvider>>,
    toolbar: Option<Box<dyn ToolbarProvider>>,
    connection: Option<Box<dyn ConnectionProvider>>,
}

impl Providers {
    pub fn new() -> Self {
        Self::default()
    }
    /// Enable payload-free request and inherited task lifecycle logging through Host.
    /// Failures remain ordinary RPC results; notifications require explicit report_error.
    pub fn diagnostics(mut self) -> Self {
        self.diagnostics = true;
        self
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
    pub fn connection_context_menu(
        mut self,
        provider: impl ConnectionContextMenuProvider + 'static,
    ) -> Self {
        self.connection_context_menu = Some(Box::new(provider));
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
    /// Register an explicit VfsProvider. Its declared ceiling and per-resource
    /// discovery control which `vfs.*` operations can be dispatched.
    pub fn filesystem<P>(mut self, provider: P) -> Self
    where
        P: VfsProvider + 'static,
    {
        self.filesystem = Some(FsProvider {
            dispatch: Box::new(provider),
            server: FsServer::new(),
        });
        self
    }

    /// `plugin.initialize` capabilities for the registered providers.
    pub fn capabilities(&self) -> Value {
        match &self.filesystem {
            Some(fs) => FsServer::capability(fs.dispatch.as_ref()),
            None => Value::Null,
        }
    }

    /// Run a provider server. Unregistered methods are delegated to `fallback`.
    pub fn serve_with<F>(self, plugin_id: &str, fallback: F) -> std::io::Result<()>
    where
        F: Fn(RequestContext) -> Result<Value, RpcError> + Send + Sync + 'static,
    {
        let capabilities = self.capabilities();
        crate::serve_declaring(plugin_id, capabilities, move |ctx| {
            let run = |ctx: RequestContext| {
                if let Some(result) = crate::diagnostics::dispatch(&ctx) {
                    return result;
                }
                match self.dispatch(&ctx)? {
                    Some(value) => Ok(value),
                    None => fallback(ctx),
                }
            };
            // Diagnostics endpoints must not log their own forwarding requests.
            if self.diagnostics && !ctx.method().starts_with("diagnostics.") {
                crate::diagnostics::request(ctx.enable_diagnostics(), run)
            } else {
                run(ctx)
            }
        })
    }

    /// Run a provider server with no custom RPC methods.
    pub fn serve(self, plugin_id: &str) -> std::io::Result<()> {
        self.serve_with(plugin_id, |ctx| {
            Err(RpcError::method_not_found(ctx.method()))
        })
    }

    fn dispatch(&self, ctx: &RequestContext) -> Result<Option<Value>, RpcError> {
        if let Some(fs) = &self.filesystem {
            if let Some(value) = fs.server.serve(fs.dispatch.as_ref(), ctx)? {
                return Ok(Some(value));
            }
        }
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
            "connectionContextMenu.resolve" => {
                let items = self
                    .connection_context_menu
                    .as_ref()
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"items":items})
            }
            "connectionContextMenu.execute" => self
                .connection_context_menu
                .as_ref()
                .ok_or_else(|| unsupported(ctx))?
                .execute(ctx)?,
            "search.query" => {
                let provider = self.search.as_ref().ok_or_else(|| unsupported(ctx))?;
                let mode: crate::filesystem::SearchMode = serde_json::from_value(
                    ctx.params().get("mode").cloned().unwrap_or(json!("name")),
                )
                .map_err(|_| RpcError::invalid_params("invalid search mode"))?;
                if !provider.supports(mode) {
                    return Err(unsupported(ctx));
                }
                serde_json::to_value(provider.query(ctx)?)
                    .map_err(|error| RpcError::internal(error.to_string()))?
            }
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
            // A connection's lifecycle methods fall through to the plugin's
            // own handler when it registers no `ConnectionProvider`.
            "connection.validate" => match self.connection.as_ref() {
                Some(provider) => provider.validate(ctx)?,
                None => return Ok(None),
            },
            "connection.open" => match self.connection.as_ref() {
                Some(provider) => provider.open(ctx)?,
                None => return Ok(None),
            },
            "connection.create" => match self.connection.as_ref() {
                Some(provider) => provider.create(ctx)?,
                None => return Ok(None),
            },
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InfoRow, InfoSection};

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
