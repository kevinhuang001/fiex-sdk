use super::*;
use crate::filesystem::{FsServer, PreflightHandlers, VfsProvider};

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
        let preflight_handlers = match provider.preflight_handlers() {
            Ok(handlers) => handlers,
            Err(error) => {
                self.registration_errors
                    .push(format!("filesystem preflight: {error}"));
                PreflightHandlers::new()
            }
        };
        let instance = FsProvider {
            dispatch: Box::new(provider),
            server: FsServer::new(),
            preflight_handlers,
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
}
