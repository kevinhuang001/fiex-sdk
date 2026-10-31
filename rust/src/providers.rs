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

mod registry;
use registry::ProviderSet;

/// Trait-specific SDK registry. One plugin can expose multiple distinct
/// providers of each kind, with stable IDs selected by trusted Core routing.
/// An omitted providerId is accepted only when exactly one instance exists.
struct FsProvider {
    dispatch: Box<dyn VfsProvider>,
    server: FsServer,
}
#[derive(Default)]
pub struct Providers {
    diagnostics: bool,
    registration_errors: Vec<String>,
    filesystem: Option<FsProvider>,
    info: ProviderSet<dyn InfoProvider>,
    preview: ProviderSet<dyn PreviewProvider>,
    decorations: ProviderSet<dyn DecorationProvider>,
    context_menu: ProviderSet<dyn ContextMenuProvider>,
    connection_context_menu: ProviderSet<dyn ConnectionContextMenuProvider>,
    search: ProviderSet<dyn SearchProvider>,
    hooks: ProviderSet<dyn HookProvider>,
    toolbar: ProviderSet<dyn ToolbarProvider>,
    connection: ProviderSet<dyn ConnectionProvider>,
}

impl Providers {
    pub fn new() -> Self {
        Self::default()
    }
    /// Enable payload-free request and inherited task lifecycle logging through Host.
    pub fn diagnostics(mut self) -> Self {
        self.diagnostics = true;
        self
    }
    /// Register a info provider under a distinct, stable ID.
    /// A repeated ID is rejected during server startup rather than silently
    /// replacing an existing registered provider.
    pub fn info_named(mut self, id: impl Into<String>, provider: impl InfoProvider + 'static) -> Self {
        if let Err(error) = self.info.register(id.into(), Box::new(provider)) {
            self.registration_errors.push(format!("info: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn info(self, provider: impl InfoProvider + 'static) -> Self {
        self.info_named("default", provider)
    }
    /// Register a preview provider under a distinct, stable ID.
    /// A repeated ID is rejected during server startup rather than silently
    /// replacing an existing registered provider.
    pub fn preview_named(mut self, id: impl Into<String>, provider: impl PreviewProvider + 'static) -> Self {
        if let Err(error) = self.preview.register(id.into(), Box::new(provider)) {
            self.registration_errors.push(format!("preview: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn preview(self, provider: impl PreviewProvider + 'static) -> Self {
        self.preview_named("default", provider)
    }
    /// Register a decorations provider under a distinct, stable ID.
    /// A repeated ID is rejected during server startup rather than silently
    /// replacing an existing registered provider.
    pub fn decorations_named(mut self, id: impl Into<String>, provider: impl DecorationProvider + 'static) -> Self {
        if let Err(error) = self.decorations.register(id.into(), Box::new(provider)) {
            self.registration_errors.push(format!("decorations: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn decorations(self, provider: impl DecorationProvider + 'static) -> Self {
        self.decorations_named("default", provider)
    }
    /// Register a context_menu provider under a distinct, stable ID.
    /// A repeated ID is rejected during server startup rather than silently
    /// replacing an existing registered provider.
    pub fn context_menu_named(mut self, id: impl Into<String>, provider: impl ContextMenuProvider + 'static) -> Self {
        if let Err(error) = self.context_menu.register(id.into(), Box::new(provider)) {
            self.registration_errors.push(format!("context_menu: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn context_menu(self, provider: impl ContextMenuProvider + 'static) -> Self {
        self.context_menu_named("default", provider)
    }
    /// Register a connection_context_menu provider under a distinct, stable ID.
    /// A repeated ID is rejected during server startup rather than silently
    /// replacing an existing registered provider.
    pub fn connection_context_menu_named(mut self, id: impl Into<String>, provider: impl ConnectionContextMenuProvider + 'static) -> Self {
        if let Err(error) = self.connection_context_menu.register(id.into(), Box::new(provider)) {
            self.registration_errors.push(format!("connection_context_menu: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn connection_context_menu(self, provider: impl ConnectionContextMenuProvider + 'static) -> Self {
        self.connection_context_menu_named("default", provider)
    }
    /// Register a search provider under a distinct, stable ID.
    /// A repeated ID is rejected during server startup rather than silently
    /// replacing an existing registered provider.
    pub fn search_named(mut self, id: impl Into<String>, provider: impl SearchProvider + 'static) -> Self {
        if let Err(error) = self.search.register(id.into(), Box::new(provider)) {
            self.registration_errors.push(format!("search: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn search(self, provider: impl SearchProvider + 'static) -> Self {
        self.search_named("default", provider)
    }
    /// Register a hooks provider under a distinct, stable ID.
    /// A repeated ID is rejected during server startup rather than silently
    /// replacing an existing registered provider.
    pub fn hooks_named(mut self, id: impl Into<String>, provider: impl HookProvider + 'static) -> Self {
        if let Err(error) = self.hooks.register(id.into(), Box::new(provider)) {
            self.registration_errors.push(format!("hooks: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn hooks(self, provider: impl HookProvider + 'static) -> Self {
        self.hooks_named("default", provider)
    }
    /// Register a toolbar provider under a distinct, stable ID.
    /// A repeated ID is rejected during server startup rather than silently
    /// replacing an existing registered provider.
    pub fn toolbar_named(mut self, id: impl Into<String>, provider: impl ToolbarProvider + 'static) -> Self {
        if let Err(error) = self.toolbar.register(id.into(), Box::new(provider)) {
            self.registration_errors.push(format!("toolbar: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn toolbar(self, provider: impl ToolbarProvider + 'static) -> Self {
        self.toolbar_named("default", provider)
    }
    /// Register a connection provider under a distinct, stable ID.
    /// A repeated ID is rejected during server startup rather than silently
    /// replacing an existing registered provider.
    pub fn connection_named(mut self, id: impl Into<String>, provider: impl ConnectionProvider + 'static) -> Self {
        if let Err(error) = self.connection.register(id.into(), Box::new(provider)) {
            self.registration_errors.push(format!("connection: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn connection(self, provider: impl ConnectionProvider + 'static) -> Self {
        self.connection_named("default", provider)
    }
    /// Register a VFS provider, with its original stream registry.
    pub fn filesystem<P>(mut self, provider: P) -> Self
    where
        P: VfsProvider + 'static,
    {
        if self.filesystem.is_some() {
            self.registration_errors.push("filesystem: duplicate provider".into());
        } else {
            self.filesystem = Some(FsProvider {
                dispatch: Box::new(provider),
                server: FsServer::new(),
            });
        }
        self
    }

    pub fn capabilities(&self) -> Value {
        match &self.filesystem {
            Some(fs) => FsServer::capability(fs.dispatch.as_ref()),
            None => Value::Null,
        }
    }

    /// Registration errors are fatal, not silent replacement or fallback.
    pub fn serve_with<F>(self, plugin_id: &str, fallback: F) -> std::io::Result<()>
    where
        F: Fn(RequestContext) -> Result<Value, RpcError> + Send + Sync + 'static,
    {
        if !self.registration_errors.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                self.registration_errors.join("; "),
            ));
        }
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
            if self.diagnostics && !ctx.method().starts_with("diagnostics.") {
                crate::diagnostics::request(ctx.enable_diagnostics(), run)
            } else {
                run(ctx)
            }
        })
    }

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
                let sections = self.info.select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?.resolve(ctx)?;
                json!({"sections":sections})
            }
            "preview.open" => {
                let view = self.preview.select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?.open(ctx)?;
                serde_json::to_value(view).map_err(|error| RpcError::internal(error.to_string()))?
            }
            "decorations.resolve" => {
                let decorations = self.decorations.select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?.resolve(ctx)?;
                json!({"decorations":decorations})
            }
            "contextMenu.resolve" => {
                let items = self.context_menu.select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?.resolve(ctx)?;
                json!({"items":items})
            }
            "contextMenu.execute" => self.context_menu.select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?.execute(ctx)?,
            "connectionContextMenu.resolve" => {
                let items = self.connection_context_menu.select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?.resolve(ctx)?;
                json!({"items":items})
            }
            "connectionContextMenu.execute" => self.connection_context_menu.select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?.execute(ctx)?,
            "search.query" => {
                let provider = self.search.select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?;
                let mode: crate::filesystem::SearchMode = serde_json::from_value(
                    ctx.params().get("mode").cloned().unwrap_or(json!("name")),
                ).map_err(|_| RpcError::invalid_params("invalid search mode"))?;
                if !provider.supports(mode) {
                    return Err(unsupported(ctx));
                }
                serde_json::to_value(provider.query(ctx)?)
                    .map_err(|error| RpcError::internal(error.to_string()))?
            }
            "hooks.invoke" => self.hooks.select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?.invoke(ctx)?,
            "toolbar.invoke" => self.toolbar.select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?.invoke(ctx)?,
            "connection.validate" => match self.connection.select(ctx.params())? {
                Some(provider) => provider.validate(ctx)?,
                None => return Ok(None),
            },
            "connection.open" => match self.connection.select(ctx.params())? {
                Some(provider) => provider.open(ctx)?,
                None => return Ok(None),
            },
            "connection.create" => match self.connection.select(ctx.params())? {
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
        assert!(providers.info.len() == 1);
        assert!(providers.search.len() == 1);
        assert!(providers.preview.is_empty());
    }

    #[test]
    fn multiple_named_instances_require_an_explicit_selection() {
        let providers = Providers::new()
            .info_named("metadata", ExampleInfo)
            .info_named("media", ExampleInfo);
        assert_eq!(providers.info.len(), 2);
        assert!(providers.info.select(&json!({})).is_err());
        assert!(providers.info.select(&json!({"providerId":"media"})).unwrap().is_some());
        assert!(providers.info.select(&json!({"providerId":"other"})).is_err());
    }

    #[test]
    fn duplicate_registration_does_not_overwrite_existing_provider() {
        let providers = Providers::new()
            .info_named("metadata", ExampleInfo)
            .info_named("metadata", ExampleInfo);
        assert_eq!(providers.info.len(), 1);
        assert_eq!(providers.registration_errors.len(), 1);
    }

    struct ExampleInfoSearch;
    impl SearchProvider for ExampleInfoSearch {
        fn query(&self, _ctx: &RequestContext) -> Result<SearchResults, RpcError> {
            Ok(SearchResults::default())
        }
    }
}
