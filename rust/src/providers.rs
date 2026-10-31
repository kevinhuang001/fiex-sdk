//! Optional typed event providers.
//!
//! A plugin can keep its existing `sdk::serve` dispatcher, or register only the
//! system events it implements here. Custom RPC methods still go through the
//! fallback handler. Provider implementations stay optional and composable;
//! nothing forces a plugin to implement events it does not own.
use crate::{
    filesystem::{FsServer, VfsProvider},
    RequestContext, RpcError,
};
use serde_json::{json, Value};

mod contracts;
use contracts::unsupported;
pub use contracts::{
    ConnectionContextMenuProvider, ConnectionProvider, ConnectionValidation, ContextMenuProvider,
    DecorationProvider, HookProvider, InfoProvider, PreviewProvider, SearchProvider, SearchResults,
    ToolbarProvider,
};

mod dispatch;
mod registry;
use registry::ProviderSet;

/// Trait-specific SDK registry. One plugin can expose multiple distinct
/// providers of each kind, with stable IDs selected by trusted Core routing.
/// An omitted providerId is accepted only when exactly one instance exists.
struct FsProvider {
    dispatch: Box<dyn VfsProvider>,
    server: FsServer,
}
pub struct Providers {
    diagnostics: bool,
    registration_errors: Vec<String>,
    filesystem: ProviderSet<FsProvider>,
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

impl Default for Providers {
    fn default() -> Self {
        use fiex_provider_contracts::ProviderKind;
        Self {
            diagnostics: false,
            registration_errors: Vec::new(),
            filesystem: ProviderSet::new(ProviderKind::Vfs),
            info: ProviderSet::new(ProviderKind::Info),
            preview: ProviderSet::new(ProviderKind::Preview),
            decorations: ProviderSet::new(ProviderKind::Decoration),
            context_menu: ProviderSet::new(ProviderKind::ContextMenu),
            connection_context_menu: ProviderSet::new(ProviderKind::ConnectionContextMenu),
            search: ProviderSet::new(ProviderKind::Search),
            hooks: ProviderSet::new(ProviderKind::Hook),
            toolbar: ProviderSet::new(ProviderKind::Toolbar),
            connection: ProviderSet::new(ProviderKind::Connection),
        }
    }
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
    pub fn info_named(
        mut self,
        id: impl Into<String>,
        provider: impl InfoProvider + 'static,
    ) -> Self {
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
    pub fn preview_named(
        mut self,
        id: impl Into<String>,
        provider: impl PreviewProvider + 'static,
    ) -> Self {
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
    pub fn decorations_named(
        mut self,
        id: impl Into<String>,
        provider: impl DecorationProvider + 'static,
    ) -> Self {
        if let Err(error) = self.decorations.register(id.into(), Box::new(provider)) {
            self.registration_errors
                .push(format!("decorations: {error}"));
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
    pub fn context_menu_named(
        mut self,
        id: impl Into<String>,
        provider: impl ContextMenuProvider + 'static,
    ) -> Self {
        if let Err(error) = self.context_menu.register(id.into(), Box::new(provider)) {
            self.registration_errors
                .push(format!("context_menu: {error}"));
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
    pub fn connection_context_menu_named(
        mut self,
        id: impl Into<String>,
        provider: impl ConnectionContextMenuProvider + 'static,
    ) -> Self {
        if let Err(error) = self
            .connection_context_menu
            .register(id.into(), Box::new(provider))
        {
            self.registration_errors
                .push(format!("connection_context_menu: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn connection_context_menu(
        self,
        provider: impl ConnectionContextMenuProvider + 'static,
    ) -> Self {
        self.connection_context_menu_named("default", provider)
    }
    /// Register a search provider under a distinct, stable ID.
    /// A repeated ID is rejected during server startup rather than silently
    /// replacing an existing registered provider.
    pub fn search_named(
        mut self,
        id: impl Into<String>,
        provider: impl SearchProvider + 'static,
    ) -> Self {
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
    pub fn hooks_named(
        mut self,
        id: impl Into<String>,
        provider: impl HookProvider + 'static,
    ) -> Self {
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
    pub fn toolbar_named(
        mut self,
        id: impl Into<String>,
        provider: impl ToolbarProvider + 'static,
    ) -> Self {
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
    pub fn connection_named(
        mut self,
        id: impl Into<String>,
        provider: impl ConnectionProvider + 'static,
    ) -> Self {
        if let Err(error) = self.connection.register(id.into(), Box::new(provider)) {
            self.registration_errors
                .push(format!("connection: {error}"));
        }
        self
    }
    /// Convenience for single-provider plugins.
    pub fn connection(self, provider: impl ConnectionProvider + 'static) -> Self {
        self.connection_named("default", provider)
    }
    /// Register an independently scoped VFS implementation by stable ID.
    /// Every instance owns its own read/write stream session registry; calls
    /// must select an explicit providerId when multiple are registered.
    pub fn filesystem_named<P>(mut self, id: impl Into<String>, provider: P) -> Self
    where
        P: VfsProvider + 'static,
    {
        let instance = FsProvider {
            dispatch: Box::new(provider),
            server: FsServer::new(),
        };
        if let Err(error) = self.filesystem.register(id.into(), Box::new(instance)) {
            self.registration_errors
                .push(format!("filesystem: {error}"));
        }
        self
    }

    pub fn filesystem<P>(self, provider: P) -> Self
    where
        P: VfsProvider + 'static,
    {
        self.filesystem_named("default", provider)
    }

    /// The handshake advertises the ceiling across registered VFS instances.
    /// Every selected provider is checked again by its own discover() result.
    pub fn capabilities(&self) -> Value {
        let mut operations = std::collections::BTreeSet::new();
        let mut preflight = false;
        let mut providers = Vec::new();
        for (id, fs) in self.filesystem.instances() {
            let methods = fs
                .dispatch
                .operations()
                .iter()
                .map(|op| op.as_str())
                .collect::<Vec<_>>();
            preflight |= fs.dispatch.supports_preflight();
            operations.extend(methods.iter().copied());
            providers.push(json!({
                "id": id,
                "operations": methods,
                "preflight": fs.dispatch.supports_preflight(),
            }));
        }
        if providers.is_empty() {
            Value::Null
        } else {
            json!({"vfs":{
                "discovery":true,
                "preflight":preflight,
                "operations":operations,
                "providers":providers,
            }})
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
}

#[cfg(test)]
mod tests;
